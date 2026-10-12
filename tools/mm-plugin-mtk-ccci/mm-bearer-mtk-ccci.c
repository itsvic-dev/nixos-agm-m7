/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Copyright (C) 2026 Wiktor Bryk <contact@itsvic.dev> */

#include <config.h>

#include <string.h>

#include <ModemManager.h>
#define _LIBMM_INSIDE_MM
#include <libmm-glib.h>

#include "mm-base-modem-at.h"
#include "mm-bearer-mtk-ccci.h"
#include "mm-errors-types.h"
#include "mm-log-object.h"
#include "mm-modem-helpers.h"
#include "mm-port-serial-at.h"

#define AT_TIMEOUT_SECS             10
#define ATTACH_TIMEOUT_SECS         60
#define ACTIVATE_TIMEOUT_SECS       60
#define DEACTIVATE_TIMEOUT_SECS     60
#define INTERFACE_WAIT_PERIOD_MSECS 250
#define INTERFACE_WAIT_MAX_TICKS    40

/* ccmni<n> has interface id 100 * m + n; only ccmni0 (wwan0) is exposed */
#define IFID_TO_CCMNI(ifid) ((ifid) % 100)

/* Android authtype values used by +EAPNSET */
#define AUTHTYPE_NONE         0
#define AUTHTYPE_PAP          1
#define AUTHTYPE_CHAP         2
#define AUTHTYPE_PAP_OR_CHAP  3

typedef enum {
    CONNECT_STEP_FIRST,
    CONNECT_STEP_APN_RESET,
    CONNECT_STEP_APN_AUTH,
    CONNECT_STEP_APN_PROPERTIES,
    CONNECT_STEP_INITIAL_ATTACH_APN,
    CONNECT_STEP_CHECK_ATTACH,
    CONNECT_STEP_ATTACH,
    CONNECT_STEP_ACTIVATE,
    CONNECT_STEP_LOOKUP_AID,
    CONNECT_STEP_WAIT_INTERFACE,
    CONNECT_STEP_INTERFACE_UP,
    CONNECT_STEP_PDN_STATE,
    CONNECT_STEP_IP_CONFIG,
    CONNECT_STEP_LAST,
    CONNECT_STEP_ROLLBACK,
} ConnectStep;

typedef struct {
    MMBaseModem    *modem;
    MMPortSerialAt *primary;
    MMPort         *data;
    gchar          *apn;
    gchar          *user;
    gchar          *password;
    guint           auth_type;
    ConnectStep     step;
    gboolean        attached;
    gboolean        activated;
    gint            aid;
    gint            ifid;
    gboolean        ifid_from_epdn;
    gchar          *ipv4;
    guint           mtu;
    gchar          *dns[3];
    guint           wait_id;
    guint           wait_ticks;
    GError         *saved_error;
} ConnectContext;

struct _MMBearerMtkCcciPrivate {
    ConnectContext *connect_ctx;
    gint            ifid;
    GRegex         *eif_regex;
    GRegex         *epdn_regex;
    GRegex         *pdn_act_regex;
};

G_DEFINE_TYPE_WITH_PRIVATE (MMBearerMtkCcci, mm_bearer_mtk_ccci, MM_TYPE_BROADBAND_BEARER)

static void connect_step (GTask *task);

static void
connect_context_free (ConnectContext *ctx)
{
    g_assert (!ctx->wait_id);
    g_clear_error (&ctx->saved_error);
    g_free (ctx->dns[0]);
    g_free (ctx->dns[1]);
    g_free (ctx->ipv4);
    g_free (ctx->password);
    g_free (ctx->user);
    g_free (ctx->apn);
    g_object_unref (ctx->data);
    g_object_unref (ctx->primary);
    g_object_unref (ctx->modem);
    g_slice_free (ConnectContext, ctx);
}

static gchar **
split_fields (const gchar *str)
{
    gchar **fields;
    guint   i;

    fields = g_strsplit (str, ",", -1);
    for (i = 0; fields[i]; i++)
        mm_strip_quotes (fields[i]);
    return fields;
}

static gboolean
is_usable_ipv4 (const gchar *str)
{
    g_autoptr(GInetAddress) address = NULL;

    if (!str || !str[0])
        return FALSE;
    address = g_inet_address_new_from_string (str);
    return (address &&
            g_inet_address_get_family (address) == G_SOCKET_FAMILY_IPV4 &&
            !g_inet_address_get_is_any (address));
}

static void
set_ipv4 (ConnectContext *ctx,
          const gchar    *address)
{
    if (!is_usable_ipv4 (address))
        return;
    g_free (ctx->ipv4);
    ctx->ipv4 = g_strdup (address);
}

/* +EIF: <ifid>, ifup, <addrtype>, <address>, ...  or  +EIF: <ifid>, mtu, <mtu> */
static void
eif_received (MMPortSerialAt  *port,
              GMatchInfo      *match_info,
              MMBearerMtkCcci *self)
{
    ConnectContext    *ctx = self->priv->connect_ctx;
    g_autofree gchar  *str = NULL;
    g_auto(GStrv)      fields = NULL;
    gint               ifid;
    guint              mtu;

    str = g_match_info_fetch (match_info, 1);
    mm_obj_dbg (self, "+EIF: %s", str);
    if (!ctx)
        return;

    fields = split_fields (str);
    if (g_strv_length (fields) < 2 || !mm_get_int_from_str (fields[0], &ifid))
        return;

    if (g_str_equal (fields[1], "ifup")) {
        if (!ctx->ifid_from_epdn)
            ctx->ifid = ifid;
        if (g_strv_length (fields) >= 4)
            set_ipv4 (ctx, fields[3]);
    } else if (g_str_equal (fields[1], "mtu") &&
               g_strv_length (fields) >= 3 &&
               mm_get_uint_from_str (fields[2], &mtu)) {
        ctx->mtu = mtu;
    }
}

/* +EPDN: <aid>, "new", <?>, <ifid>, <mtu>, <iptype>, "<ipv4>" */
static void
epdn_received (MMPortSerialAt  *port,
               GMatchInfo      *match_info,
               MMBearerMtkCcci *self)
{
    ConnectContext    *ctx = self->priv->connect_ctx;
    g_autofree gchar  *str = NULL;
    g_auto(GStrv)      fields = NULL;
    gint               aid;
    gint               ifid;
    guint              mtu;

    str = g_match_info_fetch (match_info, 1);
    mm_obj_dbg (self, "+EPDN: %s", str);
    if (!ctx)
        return;

    fields = split_fields (str);
    if (g_strv_length (fields) < 4 ||
        !g_str_equal (fields[1], "new") ||
        !mm_get_int_from_str (fields[0], &aid) ||
        !mm_get_int_from_str (fields[3], &ifid))
        return;

    if (ctx->aid >= 0 && aid != ctx->aid)
        return;

    ctx->aid = aid;
    ctx->ifid = ifid;
    ctx->ifid_from_epdn = TRUE;
    if (g_strv_length (fields) >= 5 && mm_get_uint_from_str (fields[4], &mtu))
        ctx->mtu = mtu;
    if (g_strv_length (fields) >= 7)
        set_ipv4 (ctx, fields[6]);
}

static void
pdn_act_received (MMPortSerialAt  *port,
                  GMatchInfo      *match_info,
                  MMBearerMtkCcci *self)
{
    ConnectContext *ctx = self->priv->connect_ctx;
    guint           aid;

    if (!mm_get_uint_from_match_info (match_info, 1, &aid))
        return;
    mm_obj_dbg (self, "PDN activated: %u", aid);
    if (ctx && ctx->aid < 0)
        ctx->aid = (gint)aid;
}

static void
set_urc_handlers (MMBearerMtkCcci *self,
                  MMPortSerialAt  *port,
                  gboolean         enable)
{
    mm_port_serial_at_add_unsolicited_msg_handler (port,
                                                   self->priv->eif_regex,
                                                   enable ? (MMPortSerialAtUnsolicitedMsgFn)eif_received : NULL,
                                                   enable ? self : NULL,
                                                   NULL);
    mm_port_serial_at_add_unsolicited_msg_handler (port,
                                                   self->priv->epdn_regex,
                                                   enable ? (MMPortSerialAtUnsolicitedMsgFn)epdn_received : NULL,
                                                   enable ? self : NULL,
                                                   NULL);
    if (enable)
        mm_port_serial_at_add_unsolicited_msg_handler (port,
                                                       self->priv->pdn_act_regex,
                                                       (MMPortSerialAtUnsolicitedMsgFn)pdn_act_received,
                                                       self,
                                                       NULL);
    else
        mm_port_serial_at_enable_unsolicited_msg_handler (port, self->priv->pdn_act_regex, FALSE);
}

static MMBearerConnectResult *
connect_3gpp_finish (MMBroadbandBearer  *self,
                     GAsyncResult       *res,
                     GError            **error)
{
    return g_task_propagate_pointer (G_TASK (res), error);
}

static void
connect_complete (GTask                 *task,
                  MMBearerConnectResult *result,
                  GError                *error)
{
    MMBearerMtkCcci *self;
    ConnectContext  *ctx;

    self = g_task_get_source_object (task);
    ctx = g_task_get_task_data (task);

    set_urc_handlers (self, ctx->primary, FALSE);
    self->priv->connect_ctx = NULL;

    if (error) {
        g_assert (!result);
        g_task_return_error (task, error);
    } else {
        self->priv->ifid = ctx->ifid;
        g_task_return_pointer (task, result, (GDestroyNotify)mm_bearer_connect_result_unref);
    }
    g_object_unref (task);
}

static void
connect_fail (GTask  *task,
              GError *error)
{
    ConnectContext *ctx;

    ctx = g_task_get_task_data (task);
    if (!ctx->activated) {
        connect_complete (task, NULL, error);
        return;
    }

    g_assert (!ctx->saved_error);
    ctx->saved_error = error;
    ctx->step = CONNECT_STEP_ROLLBACK;
    connect_step (task);
}

static void
connect_command_ready (MMBaseModem  *modem,
                       GAsyncResult *res,
                       GTask        *task)
{
    ConnectContext *ctx;
    GError         *error = NULL;

    ctx = g_task_get_task_data (task);
    if (!mm_base_modem_at_command_full_finish (modem, res, &error)) {
        g_prefix_error (&error, "connection step %u failed: ", (guint) ctx->step);
        connect_fail (task, error);
        return;
    }

    if (ctx->step == CONNECT_STEP_ACTIVATE)
        ctx->activated = TRUE;

    ctx->step++;
    connect_step (task);
}

static void
apn_setup_ready (MMBaseModem  *modem,
                 GAsyncResult *res,
                 GTask        *task)
{
    ConnectContext    *ctx;
    g_autoptr(GError)  error = NULL;

    ctx = g_task_get_task_data (task);
    if (!mm_base_modem_at_command_full_finish (modem, res, &error))
        mm_obj_dbg (g_task_get_source_object (task), "couldn't set up APN (step %u): %s",
                    (guint) ctx->step, error->message);

    ctx->step++;
    connect_step (task);
}

static void
initial_attach_apn_ready (MMBaseModem  *modem,
                          GAsyncResult *res,
                          GTask        *task)
{
    ConnectContext    *ctx;
    g_autoptr(GError)  error = NULL;

    ctx = g_task_get_task_data (task);
    if (!mm_base_modem_at_command_full_finish (modem, res, &error))
        mm_obj_dbg (g_task_get_source_object (task), "couldn't set initial attach APN: %s", error->message);

    ctx->step++;
    connect_step (task);
}

static void
check_attach_ready (MMBaseModem  *modem,
                    GAsyncResult *res,
                    GTask        *task)
{
    ConnectContext    *ctx;
    const gchar       *response;
    g_autoptr(GError)  error = NULL;

    ctx = g_task_get_task_data (task);
    response = mm_base_modem_at_command_full_finish (modem, res, &error);
    if (!response)
        mm_obj_dbg (g_task_get_source_object (task), "couldn't check PS attach state: %s", error->message);
    else
        ctx->attached = (strstr (response, "+CGATT: 1") != NULL);

    ctx->step++;
    connect_step (task);
}

static gboolean
apn_matches (const gchar *requested,
             const gchar *reported)
{
    gsize len;

    len = strlen (requested);
    return (g_ascii_strncasecmp (requested, reported, len) == 0 &&
            (reported[len] == '\0' || reported[len] == '.'));
}

/* 3GPP TS 27.007 +CGCONTRDP fields: 0 cid, 2 apn, 3 address, 5 dns1, 6 dns2, 11 ipv4 mtu */
static void
parse_cgcontrdp (ConnectContext *ctx,
                 const gchar    *response,
                 gboolean        lookup_aid)
{
    g_auto(GStrv) lines = NULL;
    guint         i;

    lines = g_strsplit_set (response, "\r\n", -1);
    for (i = 0; lines[i]; i++) {
        g_auto(GStrv) fields = NULL;
        guint         n_fields;
        gint          cid;
        guint         mtu;

        if (!g_str_has_prefix (lines[i], "+CGCONTRDP:"))
            continue;

        fields = split_fields (lines[i] + strlen ("+CGCONTRDP:"));
        n_fields = g_strv_length (fields);
        if (n_fields < 3 || !mm_get_int_from_str (fields[0], &cid))
            continue;

        if (lookup_aid) {
            if (apn_matches (ctx->apn, fields[2])) {
                ctx->aid = cid;
                return;
            }
            continue;
        }

        if (cid != ctx->aid)
            continue;

        if (!ctx->ipv4 && n_fields > 3) {
            g_auto(GStrv) octets = NULL;

            octets = g_strsplit (fields[3], ".", -1);
            if (g_strv_length (octets) >= 4) {
                g_autofree gchar *address = NULL;

                address = g_strdup_printf ("%s.%s.%s.%s", octets[0], octets[1], octets[2], octets[3]);
                set_ipv4 (ctx, address);
            }
        }
        if (n_fields > 5 && is_usable_ipv4 (fields[5]))
            ctx->dns[0] = g_strdup (fields[5]);
        if (n_fields > 6 && is_usable_ipv4 (fields[6]))
            ctx->dns[ctx->dns[0] ? 1 : 0] = g_strdup (fields[6]);
        if (n_fields > 11 && mm_get_uint_from_str (fields[11], &mtu) && mtu > 0)
            ctx->mtu = mtu;
        return;
    }
}

static void
lookup_aid_ready (MMBaseModem  *modem,
                  GAsyncResult *res,
                  GTask        *task)
{
    ConnectContext *ctx;
    const gchar    *response;
    GError         *error = NULL;

    ctx = g_task_get_task_data (task);
    response = mm_base_modem_at_command_full_finish (modem, res, &error);
    if (!response) {
        g_prefix_error (&error, "couldn't find the activated PDN: ");
        connect_complete (task, NULL, error);
        return;
    }

    parse_cgcontrdp (ctx, response, TRUE);
    if (ctx->aid < 0) {
        connect_complete (task, NULL,
                          g_error_new (MM_CORE_ERROR, MM_CORE_ERROR_FAILED,
                                       "couldn't find the activated PDN for APN '%s'", ctx->apn));
        return;
    }

    ctx->step++;
    connect_step (task);
}

static void
ip_config_ready (MMBaseModem  *modem,
                 GAsyncResult *res,
                 GTask        *task)
{
    ConnectContext    *ctx;
    const gchar       *response;
    g_autoptr(GError)  error = NULL;

    ctx = g_task_get_task_data (task);
    response = mm_base_modem_at_command_full_finish (modem, res, &error);
    if (!response)
        mm_obj_warn (g_task_get_source_object (task), "couldn't load DNS and MTU: %s", error->message);
    else
        parse_cgcontrdp (ctx, response, FALSE);

    ctx->step++;
    connect_step (task);
}

static gboolean
interface_wait_cb (GTask *task)
{
    ConnectContext *ctx;

    ctx = g_task_get_task_data (task);
    ctx->wait_id = 0;
    connect_step (task);
    return G_SOURCE_REMOVE;
}

static void
rollback_ready (MMBaseModem  *modem,
                GAsyncResult *res,
                GTask        *task)
{
    ConnectContext    *ctx;
    g_autoptr(GError)  error = NULL;

    ctx = g_task_get_task_data (task);
    if (!mm_base_modem_at_command_full_finish (modem, res, &error))
        mm_obj_warn (g_task_get_source_object (task), "couldn't deactivate PDN after failure: %s", error->message);

    connect_complete (task, NULL, g_steal_pointer (&ctx->saved_error));
}

static void
connect_command (GTask               *task,
                 const gchar         *command,
                 guint                timeout,
                 GAsyncReadyCallback  callback)
{
    ConnectContext *ctx;

    ctx = g_task_get_task_data (task);
    mm_base_modem_at_command_full (ctx->modem,
                                   MM_IFACE_PORT_AT (ctx->primary),
                                   command,
                                   timeout,
                                   FALSE,
                                   FALSE,
                                   g_task_get_cancellable (task),
                                   callback,
                                   task);
}

static MMBearerConnectResult *
build_connect_result (ConnectContext *ctx)
{
    g_autoptr(MMBearerIpConfig)  ipv4_config = NULL;
    MMBearerConnectResult       *result;

    ipv4_config = mm_bearer_ip_config_new ();
    mm_bearer_ip_config_set_method (ipv4_config, MM_BEARER_IP_METHOD_STATIC);
    mm_bearer_ip_config_set_address (ipv4_config, ctx->ipv4);
    mm_bearer_ip_config_set_prefix (ipv4_config, 32);
    if (ctx->dns[0])
        mm_bearer_ip_config_set_dns (ipv4_config, (const gchar **)ctx->dns);
    if (ctx->mtu)
        mm_bearer_ip_config_set_mtu (ipv4_config, ctx->mtu);

    result = mm_bearer_connect_result_new (ctx->data, ipv4_config, NULL);
    mm_bearer_connect_result_set_profile_id (result, ctx->aid);
    return result;
}

static void
connect_step (GTask *task)
{
    MMBearerMtkCcci  *self;
    ConnectContext   *ctx;
    g_autofree gchar *command = NULL;
    g_autofree gchar *apn = NULL;

    self = g_task_get_source_object (task);
    ctx = g_task_get_task_data (task);

    if (ctx->step != CONNECT_STEP_ROLLBACK && g_cancellable_is_cancelled (g_task_get_cancellable (task))) {
        connect_fail (task, g_error_new (G_IO_ERROR, G_IO_ERROR_CANCELLED, "connection attempt cancelled"));
        return;
    }

    apn = mm_at_quote_string (ctx->apn);

    switch (ctx->step) {
    case CONNECT_STEP_FIRST:
        set_urc_handlers (self, ctx->primary, TRUE);
        self->priv->connect_ctx = ctx;
        ctx->step++;
        /* fall-through */

    case CONNECT_STEP_APN_RESET:
        /* As the stock RIL: a bare +EAPNSET clears the table; adding an existing APN fails. */
        connect_command (task, "+EAPNSET", AT_TIMEOUT_SECS, (GAsyncReadyCallback)apn_setup_ready);
        return;

    case CONNECT_STEP_APN_AUTH: {
        g_autofree gchar *user = NULL;
        g_autofree gchar *password = NULL;

        user = mm_at_quote_string (ctx->user);
        password = mm_at_quote_string (ctx->password);
        command = g_strdup_printf ("+EAPNSET=%s,1,%s,%s", apn, user, password);
        connect_command (task, command, AT_TIMEOUT_SECS, (GAsyncReadyCallback)apn_setup_ready);
        return;
    }

    case CONNECT_STEP_APN_PROPERTIES: {
        g_autofree gchar *properties = NULL;
        g_autofree gchar *quoted = NULL;

        properties = g_strdup_printf ("type=default,supl,hipri;protocol=IP;roaming_protocol=IP;"
                                      "authtype=%u;carrier_enabled=1;max_conns=0;max_conns_time=0;"
                                      "wait_time=0;bearer_bitmask=2147352575;inactive_timer=0",
                                      ctx->auth_type);
        quoted = mm_at_quote_string (properties);
        command = g_strdup_printf ("+EAPNSET=%s,2,%s", apn, quoted);
        connect_command (task, command, AT_TIMEOUT_SECS, (GAsyncReadyCallback)apn_setup_ready);
        return;
    }

    case CONNECT_STEP_INITIAL_ATTACH_APN:
        command = g_strdup_printf ("+EIAAPN=%s,0", apn);
        connect_command (task, command, AT_TIMEOUT_SECS, (GAsyncReadyCallback)initial_attach_apn_ready);
        return;

    case CONNECT_STEP_CHECK_ATTACH:
        connect_command (task, "+CGATT?", AT_TIMEOUT_SECS, (GAsyncReadyCallback)check_attach_ready);
        return;

    case CONNECT_STEP_ATTACH:
        if (!ctx->attached) {
            mm_obj_dbg (self, "not PS attached, attaching...");
            connect_command (task, "+CGATT=1", ATTACH_TIMEOUT_SECS, (GAsyncReadyCallback)connect_command_ready);
            return;
        }
        ctx->step++;
        /* fall-through */

    case CONNECT_STEP_ACTIVATE:
        mm_obj_dbg (self, "activating PDN for APN '%s'...", ctx->apn);
        command = g_strdup_printf ("+EAPNACT=1,%s,\"default\",0", apn);
        connect_command (task, command, ACTIVATE_TIMEOUT_SECS, (GAsyncReadyCallback)connect_command_ready);
        return;

    case CONNECT_STEP_LOOKUP_AID:
        if (ctx->aid < 0) {
            mm_obj_dbg (self, "no PDN activation event, looking up the PDN...");
            connect_command (task, "+CGCONTRDP", AT_TIMEOUT_SECS, (GAsyncReadyCallback)lookup_aid_ready);
            return;
        }
        ctx->step++;
        /* fall-through */

    case CONNECT_STEP_WAIT_INTERFACE:
        if (ctx->ifid < 0 || !ctx->ipv4) {
            if (ctx->wait_ticks++ < INTERFACE_WAIT_MAX_TICKS) {
                ctx->wait_id = g_timeout_add (INTERFACE_WAIT_PERIOD_MSECS, (GSourceFunc)interface_wait_cb, task);
                return;
            }
            connect_fail (task, g_error_new (MM_CORE_ERROR, MM_CORE_ERROR_FAILED,
                                             "modem did not report the interface of PDN %d", ctx->aid));
            return;
        }
        if (IFID_TO_CCMNI (ctx->ifid) != 0) {
            connect_fail (task, g_error_new (MM_CORE_ERROR, MM_CORE_ERROR_UNSUPPORTED,
                                             "PDN %d uses ccmni%d, but only ccmni0 is supported",
                                             ctx->aid, IFID_TO_CCMNI (ctx->ifid)));
            return;
        }
        mm_obj_dbg (self, "PDN %d uses interface %d (ccmni%d), address %s",
                    ctx->aid, ctx->ifid, IFID_TO_CCMNI (ctx->ifid), ctx->ipv4);
        ctx->step++;
        /* fall-through */

    case CONNECT_STEP_INTERFACE_UP:
        /* without this confirmation the modem drops all downlink packets */
        command = g_strdup_printf ("+EIF=%d,\"ifst\",\"up\",1", ctx->ifid);
        connect_command (task, command, AT_TIMEOUT_SECS, (GAsyncReadyCallback)connect_command_ready);
        return;

    case CONNECT_STEP_PDN_STATE:
        /* The modem answers +EPDN only after the +EIF confirmation. */
        command = g_strdup_printf ("+EPDN=%d,\"ifst\",20", ctx->aid);
        connect_command (task, command, AT_TIMEOUT_SECS, (GAsyncReadyCallback)apn_setup_ready);
        return;

    case CONNECT_STEP_IP_CONFIG:
        connect_command (task, "+CGCONTRDP", AT_TIMEOUT_SECS, (GAsyncReadyCallback)ip_config_ready);
        return;

    case CONNECT_STEP_LAST:
        connect_complete (task, build_connect_result (ctx), NULL);
        return;

    case CONNECT_STEP_ROLLBACK:
        command = g_strdup_printf ("+EAPNACT=0,%d", ctx->aid);
        mm_base_modem_at_command_full (ctx->modem,
                                       MM_IFACE_PORT_AT (ctx->primary),
                                       command,
                                       DEACTIVATE_TIMEOUT_SECS,
                                       FALSE,
                                       FALSE,
                                       NULL,
                                       (GAsyncReadyCallback)rollback_ready,
                                       task);
        return;

    default:
        g_assert_not_reached ();
    }
}

static guint
auth_type_from_allowed_auth (MMBearerAllowedAuth  allowed_auth,
                             const gchar         *user,
                             const gchar         *password)
{
    gboolean pap;
    gboolean chap;

    if (allowed_auth == MM_BEARER_ALLOWED_AUTH_UNKNOWN)
        return (user || password) ? AUTHTYPE_CHAP : AUTHTYPE_NONE;
    if (allowed_auth & MM_BEARER_ALLOWED_AUTH_NONE)
        return AUTHTYPE_NONE;

    pap = !!(allowed_auth & MM_BEARER_ALLOWED_AUTH_PAP);
    chap = !!(allowed_auth & MM_BEARER_ALLOWED_AUTH_CHAP);
    if (pap && chap)
        return AUTHTYPE_PAP_OR_CHAP;
    if (pap)
        return AUTHTYPE_PAP;
    return AUTHTYPE_CHAP;
}

static void
connect_3gpp (MMBroadbandBearer   *_self,
              MMBroadbandModem    *modem,
              MMPortSerialAt      *primary,
              MMPortSerialAt      *secondary,
              GCancellable        *cancellable,
              GAsyncReadyCallback  callback,
              gpointer             user_data)
{
    MMBearerMtkCcci    *self = MM_BEARER_MTK_CCCI (_self);
    MMBearerProperties *config;
    MMBearerIpFamily    ip_type;
    MMPort             *data;
    ConnectContext     *ctx;
    GTask              *task;
    const gchar        *apn;
    const gchar        *user;
    const gchar        *password;

    task = g_task_new (self, cancellable, callback, user_data);

    if (self->priv->connect_ctx) {
        g_task_return_new_error (task, MM_CORE_ERROR, MM_CORE_ERROR_IN_PROGRESS,
                                 "Couldn't connect: connection attempt already in progress");
        g_object_unref (task);
        return;
    }

    config = mm_base_bearer_peek_config (MM_BASE_BEARER (self));
    apn = mm_bearer_properties_get_apn (config);
    if (!apn || !apn[0]) {
        g_task_return_new_error (task, MM_CORE_ERROR, MM_CORE_ERROR_INVALID_ARGS,
                                 "Couldn't connect: an APN is required");
        g_object_unref (task);
        return;
    }

    ip_type = mm_bearer_properties_get_ip_type (config);
    if (ip_type != MM_BEARER_IP_FAMILY_NONE &&
        ip_type != MM_BEARER_IP_FAMILY_ANY &&
        !(ip_type & (MM_BEARER_IP_FAMILY_IPV4 | MM_BEARER_IP_FAMILY_IPV4V6))) {
        g_autofree gchar *ip_type_str = NULL;

        ip_type_str = mm_bearer_ip_family_build_string_from_mask (ip_type);
        g_task_return_new_error (task, MM_CORE_ERROR, MM_CORE_ERROR_UNSUPPORTED,
                                 "Couldn't connect: IP type '%s' is not supported", ip_type_str);
        g_object_unref (task);
        return;
    }

    data = mm_base_modem_peek_best_data_port (MM_BASE_MODEM (modem), MM_PORT_TYPE_NET);
    if (!data) {
        g_task_return_new_error (task, MM_CORE_ERROR, MM_CORE_ERROR_NOT_FOUND,
                                 "Couldn't connect: no net port available");
        g_object_unref (task);
        return;
    }

    user = mm_bearer_properties_get_user (config);
    password = mm_bearer_properties_get_password (config);

    ctx = g_slice_new0 (ConnectContext);
    ctx->modem = MM_BASE_MODEM (g_object_ref (modem));
    ctx->primary = g_object_ref (primary);
    ctx->data = g_object_ref (data);
    ctx->apn = g_strdup (apn);
    ctx->user = g_strdup (user);
    ctx->password = g_strdup (password);
    ctx->auth_type = auth_type_from_allowed_auth (mm_bearer_properties_get_allowed_auth (config), user, password);
    ctx->step = CONNECT_STEP_FIRST;
    ctx->aid = -1;
    ctx->ifid = -1;
    g_task_set_task_data (task, ctx, (GDestroyNotify)connect_context_free);

    connect_step (task);
}

typedef struct {
    MMBaseModem    *modem;
    MMPortSerialAt *primary;
    gint            ifid;
} DisconnectContext;

static void
disconnect_context_free (DisconnectContext *ctx)
{
    g_object_unref (ctx->primary);
    g_object_unref (ctx->modem);
    g_slice_free (DisconnectContext, ctx);
}

static gboolean
disconnect_3gpp_finish (MMBroadbandBearer  *self,
                        GAsyncResult       *res,
                        GError            **error)
{
    return g_task_propagate_boolean (G_TASK (res), error);
}

static void
interface_down_ready (MMBaseModem  *modem,
                      GAsyncResult *res,
                      GTask        *task)
{
    g_autoptr(GError) error = NULL;

    if (!mm_base_modem_at_command_full_finish (modem, res, &error))
        mm_obj_dbg (g_task_get_source_object (task), "couldn't confirm interface down: %s", error->message);

    g_task_return_boolean (task, TRUE);
    g_object_unref (task);
}

static void
deactivate_ready (MMBaseModem  *modem,
                  GAsyncResult *res,
                  GTask        *task)
{
    DisconnectContext *ctx;
    g_autoptr(GError)  error = NULL;
    g_autofree gchar  *command = NULL;

    ctx = g_task_get_task_data (task);
    if (!mm_base_modem_at_command_full_finish (modem, res, &error))
        mm_obj_warn (g_task_get_source_object (task), "couldn't deactivate PDN: %s", error->message);

    if (ctx->ifid < 0) {
        g_task_return_boolean (task, TRUE);
        g_object_unref (task);
        return;
    }

    command = g_strdup_printf ("+EIF=%d,\"ifst\",\"down\",1", ctx->ifid);
    mm_base_modem_at_command_full (ctx->modem,
                                   MM_IFACE_PORT_AT (ctx->primary),
                                   command,
                                   AT_TIMEOUT_SECS,
                                   FALSE,
                                   FALSE,
                                   NULL,
                                   (GAsyncReadyCallback)interface_down_ready,
                                   task);
}

static void
disconnect_3gpp (MMBroadbandBearer   *_self,
                 MMBroadbandModem    *modem,
                 MMPortSerialAt      *primary,
                 MMPortSerialAt      *secondary,
                 MMPort              *data,
                 guint                cid,
                 GAsyncReadyCallback  callback,
                 gpointer             user_data)
{
    MMBearerMtkCcci   *self = MM_BEARER_MTK_CCCI (_self);
    DisconnectContext *ctx;
    GTask             *task;
    g_autofree gchar  *command = NULL;

    task = g_task_new (self, NULL, callback, user_data);

    ctx = g_slice_new0 (DisconnectContext);
    ctx->modem = MM_BASE_MODEM (g_object_ref (modem));
    ctx->primary = g_object_ref (primary);
    ctx->ifid = self->priv->ifid;
    g_task_set_task_data (task, ctx, (GDestroyNotify)disconnect_context_free);

    self->priv->ifid = -1;

    command = g_strdup_printf ("+EAPNACT=0,%u", cid);
    mm_base_modem_at_command_full (ctx->modem,
                                   MM_IFACE_PORT_AT (ctx->primary),
                                   command,
                                   DEACTIVATE_TIMEOUT_SECS,
                                   FALSE,
                                   FALSE,
                                   NULL,
                                   (GAsyncReadyCallback)deactivate_ready,
                                   task);
}

MMBaseBearer *
mm_bearer_mtk_ccci_new_finish (GAsyncResult  *res,
                               GError       **error)
{
    GObject *bearer;
    GObject *source;

    source = g_async_result_get_source_object (res);
    bearer = g_async_initable_new_finish (G_ASYNC_INITABLE (source), res, error);
    g_object_unref (source);

    if (!bearer)
        return NULL;

    mm_base_bearer_export (MM_BASE_BEARER (bearer));
    return MM_BASE_BEARER (bearer);
}

void
mm_bearer_mtk_ccci_new (MMBroadbandModem    *modem,
                        MMBearerProperties  *config,
                        GCancellable        *cancellable,
                        GAsyncReadyCallback  callback,
                        gpointer             user_data)
{
    g_async_initable_new_async (MM_TYPE_BEARER_MTK_CCCI,
                                G_PRIORITY_DEFAULT,
                                cancellable,
                                callback,
                                user_data,
                                MM_BASE_BEARER_MODEM,  modem,
                                MM_BASE_BEARER_CONFIG, config,
                                NULL);
}

static void
mm_bearer_mtk_ccci_init (MMBearerMtkCcci *self)
{
    self->priv = mm_bearer_mtk_ccci_get_instance_private (self);
    self->priv->ifid = -1;

    self->priv->eif_regex = g_regex_new (MM_MTK_CCCI_EIF_URC_PATTERN,
                                         G_REGEX_RAW | G_REGEX_OPTIMIZE, 0, NULL);
    self->priv->epdn_regex = g_regex_new (MM_MTK_CCCI_EPDN_URC_PATTERN,
                                          G_REGEX_RAW | G_REGEX_OPTIMIZE, 0, NULL);
    self->priv->pdn_act_regex = g_regex_new ("\\r\\n\\+CGEV:\\s*ME PDN ACT\\s*(\\d+)[^\\r\\n]*\\r\\n",
                                             G_REGEX_RAW | G_REGEX_OPTIMIZE, 0, NULL);
    g_assert (self->priv->eif_regex);
    g_assert (self->priv->epdn_regex);
    g_assert (self->priv->pdn_act_regex);
}

static void
finalize (GObject *object)
{
    MMBearerMtkCcci *self = MM_BEARER_MTK_CCCI (object);

    g_assert (!self->priv->connect_ctx);
    g_regex_unref (self->priv->eif_regex);
    g_regex_unref (self->priv->epdn_regex);
    g_regex_unref (self->priv->pdn_act_regex);

    G_OBJECT_CLASS (mm_bearer_mtk_ccci_parent_class)->finalize (object);
}

static void
mm_bearer_mtk_ccci_class_init (MMBearerMtkCcciClass *klass)
{
    GObjectClass           *object_class = G_OBJECT_CLASS (klass);
    MMBaseBearerClass      *base_bearer_class = MM_BASE_BEARER_CLASS (klass);
    MMBroadbandBearerClass *broadband_bearer_class = MM_BROADBAND_BEARER_CLASS (klass);

    object_class->finalize = finalize;

    /* +CGACT? is not known to report +EAPNACT PDNs; drops come in as +CGEV */
    base_bearer_class->load_connection_status = NULL;
    base_bearer_class->load_connection_status_finish = NULL;
#if defined WITH_SUSPEND_RESUME
    base_bearer_class->reload_connection_status = NULL;
    base_bearer_class->reload_connection_status_finish = NULL;
#endif

    broadband_bearer_class->connect_3gpp = connect_3gpp;
    broadband_bearer_class->connect_3gpp_finish = connect_3gpp_finish;
    broadband_bearer_class->disconnect_3gpp = disconnect_3gpp;
    broadband_bearer_class->disconnect_3gpp_finish = disconnect_3gpp_finish;
}
