/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Copyright (C) 2026 Wiktor Bryk <contact@itsvic.dev> */

#include <config.h>

#include <string.h>

#include "ModemManager.h"
#include "mm-log-object.h"
#include "mm-errors-types.h"
#include "mm-modem-helpers.h"
#include "mm-base-modem-at.h"
#include "mm-iface-modem.h"
#include "mm-iface-modem-3gpp.h"
#include "mm-broadband-modem-mtk-ccci.h"
#include "mm-bearer-mtk-ccci.h"

static void iface_modem_init      (MMIfaceModemInterface     *iface);
static void iface_modem_3gpp_init (MMIfaceModem3gppInterface *iface);

static MMIfaceModemInterface     *iface_modem_parent;
static MMIfaceModem3gppInterface *iface_modem_3gpp_parent;

struct _MMBroadbandModemMtkCcciPrivate {
    GRegex *vendor_urc_regex;
    GRegex *eif_regex;
    GRegex *epdn_regex;
    gboolean sim_powered;
};

G_DEFINE_TYPE_EXTENDED (MMBroadbandModemMtkCcci, mm_broadband_modem_mtk_ccci, MM_TYPE_BROADBAND_MODEM, 0,
                        G_ADD_PRIVATE (MMBroadbandModemMtkCcci)
                        G_IMPLEMENT_INTERFACE (MM_TYPE_IFACE_MODEM,      iface_modem_init)
                        G_IMPLEMENT_INTERFACE (MM_TYPE_IFACE_MODEM_3GPP, iface_modem_3gpp_init))

static MMBaseBearer *
modem_create_bearer_finish (MMIfaceModem  *self,
                            GAsyncResult  *res,
                            GError       **error)
{
    return g_task_propagate_pointer (G_TASK (res), error);
}

static void
bearer_new_ready (GObject      *source,
                  GAsyncResult *res,
                  GTask        *task)
{
    MMBaseBearer *bearer;
    GError       *error = NULL;

    bearer = mm_bearer_mtk_ccci_new_finish (res, &error);
    if (!bearer)
        g_task_return_error (task, error);
    else
        g_task_return_pointer (task, bearer, g_object_unref);
    g_object_unref (task);
}

static void
modem_create_bearer (MMIfaceModem        *self,
                     MMBearerProperties  *properties,
                     GAsyncReadyCallback  callback,
                     gpointer             user_data)
{
    mm_bearer_mtk_ccci_new (MM_BROADBAND_MODEM (self),
                            properties,
                            NULL,
                            (GAsyncReadyCallback)bearer_new_ready,
                            g_task_new (self, NULL, callback, user_data));
}

static MMModemCapability
load_current_capabilities_finish (MMIfaceModem  *self,
                                  GAsyncResult  *res,
                                  GError       **error)
{
    GError *inner_error = NULL;
    gssize  value;

    value = g_task_propagate_int (G_TASK (res), &inner_error);
    if (inner_error) {
        g_propagate_error (error, inner_error);
        return MM_MODEM_CAPABILITY_NONE;
    }
    return (MMModemCapability)value;
}

static void
load_current_capabilities (MMIfaceModem        *self,
                           GAsyncReadyCallback  callback,
                           gpointer             user_data)
{
    GTask *task;

    task = g_task_new (self, NULL, callback, user_data);
    g_task_return_int (task, MM_MODEM_CAPABILITY_GSM_UMTS | MM_MODEM_CAPABILITY_LTE);
    g_object_unref (task);
}

typedef struct {
    gint        erat_mode;
    gint        erat_pref;
    MMModemMode allowed;
    MMModemMode preferred;
} EratMode;

/* +ERAT preference 4 is "LTE preferred" on MOLY.LR12A; the stock RIL sets 6,4 */
static const EratMode erat_modes[] = {
    { 0, 0, MM_MODEM_MODE_2G,                                       MM_MODEM_MODE_NONE },
    { 1, 0, MM_MODEM_MODE_3G,                                       MM_MODEM_MODE_NONE },
    { 2, 0, MM_MODEM_MODE_2G | MM_MODEM_MODE_3G,                    MM_MODEM_MODE_NONE },
    { 2, 2, MM_MODEM_MODE_2G | MM_MODEM_MODE_3G,                    MM_MODEM_MODE_3G   },
    { 3, 0, MM_MODEM_MODE_4G,                                       MM_MODEM_MODE_NONE },
    { 4, 0, MM_MODEM_MODE_2G | MM_MODEM_MODE_4G,                    MM_MODEM_MODE_NONE },
    { 5, 0, MM_MODEM_MODE_3G | MM_MODEM_MODE_4G,                    MM_MODEM_MODE_NONE },
    { 6, 0, MM_MODEM_MODE_2G | MM_MODEM_MODE_3G | MM_MODEM_MODE_4G, MM_MODEM_MODE_NONE },
    { 6, 4, MM_MODEM_MODE_2G | MM_MODEM_MODE_3G | MM_MODEM_MODE_4G, MM_MODEM_MODE_4G   },
};

static GArray *
load_supported_modes_finish (MMIfaceModem  *self,
                             GAsyncResult  *res,
                             GError       **error)
{
    return g_task_propagate_pointer (G_TASK (res), error);
}

static void
load_supported_modes (MMIfaceModem        *self,
                      GAsyncReadyCallback  callback,
                      gpointer             user_data)
{
    GTask  *task;
    GArray *combinations;
    guint   i;

    task = g_task_new (self, NULL, callback, user_data);

    combinations = g_array_sized_new (FALSE, FALSE, sizeof (MMModemModeCombination), G_N_ELEMENTS (erat_modes));
    for (i = 0; i < G_N_ELEMENTS (erat_modes); i++) {
        MMModemModeCombination mode;

        mode.allowed = erat_modes[i].allowed;
        mode.preferred = erat_modes[i].preferred;
        g_array_append_val (combinations, mode);
    }

    g_task_return_pointer (task, combinations, (GDestroyNotify)g_array_unref);
    g_object_unref (task);
}

static gboolean
load_current_modes_finish (MMIfaceModem  *self,
                           GAsyncResult  *res,
                           MMModemMode   *allowed,
                           MMModemMode   *preferred,
                           GError       **error)
{
    g_autoptr(GRegex)      r = NULL;
    g_autoptr(GMatchInfo)  match_info = NULL;
    const gchar           *response;
    gint                   erat_mode = -1;
    gint                   erat_pref = -1;

    response = mm_base_modem_at_command_finish (MM_BASE_MODEM (self), res, error);
    if (!response)
        return FALSE;

    r = g_regex_new ("\\+ERAT:\\s*\\d+,\\s*\\d+,\\s*(\\d+),\\s*(\\d+)", G_REGEX_RAW, 0, NULL);
    g_assert (r != NULL);

    if (!g_regex_match (r, response, 0, &match_info) ||
        !mm_get_int_from_match_info (match_info, 1, &erat_mode) ||
        !mm_get_int_from_match_info (match_info, 2, &erat_pref)) {
        g_set_error (error, MM_CORE_ERROR, MM_CORE_ERROR_FAILED,
                     "Couldn't parse +ERAT response: '%s'", response);
        return FALSE;
    }

    switch (erat_mode) {
    case 0: *allowed = MM_MODEM_MODE_2G; break;
    case 1: *allowed = MM_MODEM_MODE_3G; break;
    case 2: *allowed = MM_MODEM_MODE_2G | MM_MODEM_MODE_3G; break;
    case 3: *allowed = MM_MODEM_MODE_4G; break;
    case 4: *allowed = MM_MODEM_MODE_2G | MM_MODEM_MODE_4G; break;
    case 5: *allowed = MM_MODEM_MODE_3G | MM_MODEM_MODE_4G; break;
    case 6: *allowed = MM_MODEM_MODE_2G | MM_MODEM_MODE_3G | MM_MODEM_MODE_4G; break;
    default:
        g_set_error (error, MM_CORE_ERROR, MM_CORE_ERROR_FAILED,
                     "Unknown +ERAT mode: %d", erat_mode);
        return FALSE;
    }

    switch (erat_pref) {
    case 0: *preferred = MM_MODEM_MODE_NONE; break;
    case 1: *preferred = MM_MODEM_MODE_2G; break;
    case 2: *preferred = MM_MODEM_MODE_3G; break;
    case 3:
    case 4: *preferred = MM_MODEM_MODE_4G; break;
    default:
        g_set_error (error, MM_CORE_ERROR, MM_CORE_ERROR_FAILED,
                     "Unknown +ERAT preference: %d", erat_pref);
        return FALSE;
    }

    return TRUE;
}

static void
load_current_modes (MMIfaceModem        *self,
                    GAsyncReadyCallback  callback,
                    gpointer             user_data)
{
    mm_base_modem_at_command (MM_BASE_MODEM (self),
                              "+ERAT?",
                              5,
                              FALSE,
                              callback,
                              user_data);
}

static gboolean
set_current_modes_finish (MMIfaceModem  *self,
                          GAsyncResult  *res,
                          GError       **error)
{
    return g_task_propagate_boolean (G_TASK (res), error);
}

static void
erat_set_ready (MMBaseModem  *self,
                GAsyncResult *res,
                GTask        *task)
{
    GError *error = NULL;

    if (!mm_base_modem_at_command_finish (self, res, &error))
        g_task_return_error (task, error);
    else
        g_task_return_boolean (task, TRUE);
    g_object_unref (task);
}

static void
set_current_modes (MMIfaceModem        *self,
                   MMModemMode          allowed,
                   MMModemMode          preferred,
                   GAsyncReadyCallback  callback,
                   gpointer             user_data)
{
    GTask            *task;
    g_autofree gchar *command = NULL;
    guint             i;

    task = g_task_new (self, NULL, callback, user_data);

    for (i = 0; i < G_N_ELEMENTS (erat_modes); i++) {
        if (erat_modes[i].allowed == allowed && erat_modes[i].preferred == preferred)
            break;
    }

    if (i == G_N_ELEMENTS (erat_modes)) {
        g_autofree gchar *allowed_str = NULL;
        g_autofree gchar *preferred_str = NULL;

        allowed_str = mm_modem_mode_build_string_from_mask (allowed);
        preferred_str = mm_modem_mode_build_string_from_mask (preferred);
        g_task_return_new_error (task, MM_CORE_ERROR, MM_CORE_ERROR_UNSUPPORTED,
                                 "Requested mode (allowed: '%s', preferred: '%s') not supported by the modem",
                                 allowed_str, preferred_str);
        g_object_unref (task);
        return;
    }

    command = g_strdup_printf ("+ERAT=%d,%d", erat_modes[i].erat_mode, erat_modes[i].erat_pref);
    mm_base_modem_at_command (MM_BASE_MODEM (self),
                              command,
                              30,
                              FALSE,
                              (GAsyncReadyCallback)erat_set_ready,
                              task);
}

/* The SIM is off at power-up; the stock RIL turns it on with +EFUN=0. */

static MMModemLock
load_unlock_required_finish (MMIfaceModem  *self,
                             GAsyncResult  *res,
                             GError       **error)
{
    GError *inner_error = NULL;
    gssize  value;

    value = g_task_propagate_int (G_TASK (res), &inner_error);
    if (inner_error) {
        g_propagate_error (error, inner_error);
        return MM_MODEM_LOCK_UNKNOWN;
    }
    return (MMModemLock) value;
}

static void
parent_load_unlock_required_ready (MMIfaceModem *self,
                                   GAsyncResult *res,
                                   GTask        *task)
{
    GError      *error = NULL;
    MMModemLock  lock;

    lock = iface_modem_parent->load_unlock_required_finish (self, res, &error);
    if (error)
        g_task_return_error (task, error);
    else
        g_task_return_int (task, lock);
    g_object_unref (task);
}

static void
run_parent_load_unlock_required (GTask *task)
{
    MMIfaceModem *self = g_task_get_source_object (task);

    iface_modem_parent->load_unlock_required (self,
                                              GPOINTER_TO_UINT (g_task_get_task_data (task)),
                                              g_task_get_cancellable (task),
                                              (GAsyncReadyCallback) parent_load_unlock_required_ready,
                                              task);
}

static gboolean
sim_settle_cb (GTask *task)
{
    run_parent_load_unlock_required (task);
    return G_SOURCE_REMOVE;
}

static MMBaseModemAtResponseProcessorResult
response_processor_warn_and_continue (MMBaseModem   *self,
                                      gpointer       none,
                                      const gchar   *command,
                                      const gchar   *response,
                                      gboolean       last_command,
                                      const GError  *error,
                                      GVariant     **result,
                                      GError       **result_error);

static const MMBaseModemAtCommand sim_power_sequence[] = {
    { "+EBOOT=0", 5, FALSE, response_processor_warn_and_continue },
    { "+EFUN=0",  10, FALSE, response_processor_warn_and_continue },
    { NULL }
};

static void
sim_power_ready (MMBaseModem  *self,
                 GAsyncResult *res,
                 GTask        *task)
{
    mm_base_modem_at_sequence_finish (self, res, NULL, NULL);
    MM_BROADBAND_MODEM_MTK_CCCI (self)->priv->sim_powered = TRUE;
    g_timeout_add_seconds (3, (GSourceFunc) sim_settle_cb, task);
}

static void
load_unlock_required (MMIfaceModem        *self,
                      gboolean             last_attempt,
                      GCancellable        *cancellable,
                      GAsyncReadyCallback  callback,
                      gpointer             user_data)
{
    GTask *task;

    task = g_task_new (self, cancellable, callback, user_data);
    g_task_set_task_data (task, GUINT_TO_POINTER (last_attempt), NULL);

    if (MM_BROADBAND_MODEM_MTK_CCCI (self)->priv->sim_powered) {
        run_parent_load_unlock_required (task);
        return;
    }
    mm_base_modem_at_sequence (MM_BASE_MODEM (self),
                               sim_power_sequence,
                               NULL, NULL,
                               (GAsyncReadyCallback) sim_power_ready,
                               task);
}

static MMBaseModemAtResponseProcessorResult
response_processor_warn_and_continue (MMBaseModem   *self,
                                      gpointer       none,
                                      const gchar   *command,
                                      const gchar   *response,
                                      gboolean       last_command,
                                      const GError  *error,
                                      GVariant     **result,
                                      GError       **result_error)
{
    if (error)
        mm_obj_warn (self, "data setup command '%s' failed: %s", command, error->message);
    *result = NULL;
    *result_error = NULL;
    return MM_BASE_MODEM_AT_RESPONSE_PROCESSOR_RESULT_CONTINUE;
}

/* Same order as the stock RIL; without them the PDN does not carry data */
static const MMBaseModemAtCommand data_setup_sequence[] = {
    { "+EIFCONFIG=3", 5, FALSE, response_processor_warn_and_continue },
    { "+EIAREG=1",    5, FALSE, response_processor_warn_and_continue },
    { "+EDALLOW=1",   5, FALSE, response_processor_warn_and_continue },
    { NULL }
};

static gboolean
modem_3gpp_enable_unsolicited_events_finish (MMIfaceModem3gpp  *self,
                                             GAsyncResult      *res,
                                             GError           **error)
{
    return g_task_propagate_boolean (G_TASK (res), error);
}

static void
data_setup_ready (MMBaseModem  *self,
                  GAsyncResult *res,
                  GTask        *task)
{
    GError *error = NULL;

    mm_base_modem_at_sequence_finish (self, res, NULL, &error);
    if (error)
        g_task_return_error (task, error);
    else
        g_task_return_boolean (task, TRUE);
    g_object_unref (task);
}

static void
parent_enable_unsolicited_events_ready (MMIfaceModem3gpp *self,
                                        GAsyncResult     *res,
                                        GTask            *task)
{
    g_autoptr(GError) error = NULL;

    if (!iface_modem_3gpp_parent->enable_unsolicited_events_finish (self, res, &error))
        mm_obj_dbg (self, "enabling generic unsolicited events failed: %s", error->message);

    mm_base_modem_at_sequence (MM_BASE_MODEM (self),
                               data_setup_sequence,
                               NULL,
                               NULL,
                               (GAsyncReadyCallback)data_setup_ready,
                               task);
}

static void
modem_3gpp_enable_unsolicited_events (MMIfaceModem3gpp    *self,
                                      GAsyncReadyCallback  callback,
                                      gpointer             user_data)
{
    iface_modem_3gpp_parent->enable_unsolicited_events (
        self,
        (GAsyncReadyCallback)parent_enable_unsolicited_events_ready,
        g_task_new (self, NULL, callback, user_data));
}

static void
setup_ports (MMBroadbandModem *_self)
{
    MMBroadbandModemMtkCcci *self = MM_BROADBAND_MODEM_MTK_CCCI (_self);
    MMPortSerialAt          *ports[2];
    guint                    i;

    MM_BROADBAND_MODEM_CLASS (mm_broadband_modem_mtk_ccci_parent_class)->setup_ports (_self);

    ports[0] = mm_base_modem_peek_port_primary (MM_BASE_MODEM (self));
    ports[1] = mm_base_modem_peek_port_secondary (MM_BASE_MODEM (self));

    for (i = 0; i < G_N_ELEMENTS (ports); i++) {
        if (!ports[i])
            continue;
        mm_port_serial_at_add_unsolicited_msg_handler (ports[i], self->priv->vendor_urc_regex, NULL, NULL, NULL);
        mm_port_serial_at_add_unsolicited_msg_handler (ports[i], self->priv->eif_regex, NULL, NULL, NULL);
        mm_port_serial_at_add_unsolicited_msg_handler (ports[i], self->priv->epdn_regex, NULL, NULL, NULL);
    }
}

MMBroadbandModemMtkCcci *
mm_broadband_modem_mtk_ccci_new (const gchar  *device,
                                 const gchar  *physdev,
                                 const gchar **drivers,
                                 const gchar  *plugin,
                                 guint16       vendor_id,
                                 guint16       product_id)
{
    return g_object_new (MM_TYPE_BROADBAND_MODEM_MTK_CCCI,
                         MM_BASE_MODEM_DEVICE,             device,
                         MM_BASE_MODEM_PHYSDEV,            physdev,
                         MM_BASE_MODEM_DRIVERS,            drivers,
                         MM_BASE_MODEM_PLUGIN,             plugin,
                         MM_BASE_MODEM_VENDOR_ID,          vendor_id,
                         MM_BASE_MODEM_PRODUCT_ID,         product_id,
                         MM_BASE_MODEM_DATA_NET_SUPPORTED, TRUE,
                         MM_BASE_MODEM_DATA_TTY_SUPPORTED, FALSE,
                         NULL);
}

static void
mm_broadband_modem_mtk_ccci_init (MMBroadbandModemMtkCcci *self)
{
    self->priv = mm_broadband_modem_mtk_ccci_get_instance_private (self);

    self->priv->vendor_urc_regex = g_regex_new ("\\r\\n\\+(?:EIND|ESIMIND|EUSIM|ECPIN|ETESTSIM|EIRAT|"
                                                "CEN1|CEN2|CNEMS1|CNEMIU|CIREPI|EDRETRY|EWIFIPDNACT|"
                                                "EAPNINFO|EIMSPDIS|ECSQ):[^\\r\\n]*\\r\\n",
                                                G_REGEX_RAW | G_REGEX_OPTIMIZE, 0, NULL);
    self->priv->eif_regex = g_regex_new (MM_MTK_CCCI_EIF_URC_PATTERN,
                                         G_REGEX_RAW | G_REGEX_OPTIMIZE, 0, NULL);
    self->priv->epdn_regex = g_regex_new (MM_MTK_CCCI_EPDN_URC_PATTERN,
                                          G_REGEX_RAW | G_REGEX_OPTIMIZE, 0, NULL);
    g_assert (self->priv->vendor_urc_regex);
    g_assert (self->priv->eif_regex);
    g_assert (self->priv->epdn_regex);
}

static void
finalize (GObject *object)
{
    MMBroadbandModemMtkCcci *self = MM_BROADBAND_MODEM_MTK_CCCI (object);

    g_regex_unref (self->priv->vendor_urc_regex);
    g_regex_unref (self->priv->eif_regex);
    g_regex_unref (self->priv->epdn_regex);

    G_OBJECT_CLASS (mm_broadband_modem_mtk_ccci_parent_class)->finalize (object);
}

static void
iface_modem_init (MMIfaceModemInterface *iface)
{
    iface_modem_parent = g_type_interface_peek_parent (iface);

    iface->load_unlock_required = load_unlock_required;
    iface->load_unlock_required_finish = load_unlock_required_finish;
    iface->create_bearer = modem_create_bearer;
    iface->create_bearer_finish = modem_create_bearer_finish;
    iface->load_current_capabilities = load_current_capabilities;
    iface->load_current_capabilities_finish = load_current_capabilities_finish;
    iface->load_supported_modes = load_supported_modes;
    iface->load_supported_modes_finish = load_supported_modes_finish;
    iface->load_current_modes = load_current_modes;
    iface->load_current_modes_finish = load_current_modes_finish;
    iface->set_current_modes = set_current_modes;
    iface->set_current_modes_finish = set_current_modes_finish;
}

static void
iface_modem_3gpp_init (MMIfaceModem3gppInterface *iface)
{
    iface_modem_3gpp_parent = g_type_interface_peek_parent (iface);

    iface->enable_unsolicited_events = modem_3gpp_enable_unsolicited_events;
    iface->enable_unsolicited_events_finish = modem_3gpp_enable_unsolicited_events_finish;
}

static void
mm_broadband_modem_mtk_ccci_class_init (MMBroadbandModemMtkCcciClass *klass)
{
    GObjectClass          *object_class = G_OBJECT_CLASS (klass);
    MMBroadbandModemClass *broadband_modem_class = MM_BROADBAND_MODEM_CLASS (klass);

    object_class->finalize = finalize;
    broadband_modem_class->setup_ports = setup_ports;
}
