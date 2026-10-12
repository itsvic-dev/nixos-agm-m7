/* SPDX-License-Identifier: GPL-2.0-or-later */
/* Copyright (C) 2026 Wiktor Bryk <contact@itsvic.dev> */

#ifndef MM_BEARER_MTK_CCCI_H
#define MM_BEARER_MTK_CCCI_H

#include <glib.h>
#include <glib-object.h>

#define _LIBMM_INSIDE_MM
#include <libmm-glib.h>

#include "mm-broadband-bearer.h"
#include "mm-broadband-modem.h"

/* URC handlers are matched by pattern, so the modem and the bearer must share these */
#define MM_MTK_CCCI_EIF_URC_PATTERN  "\\r\\n\\+EIF:\\s*([^\\r\\n]*)\\r\\n"
#define MM_MTK_CCCI_EPDN_URC_PATTERN "\\r\\n\\+EPDN:\\s*([^\\r\\n]*)\\r\\n"

#define MM_TYPE_BEARER_MTK_CCCI            (mm_bearer_mtk_ccci_get_type ())
#define MM_BEARER_MTK_CCCI(obj)            (G_TYPE_CHECK_INSTANCE_CAST ((obj), MM_TYPE_BEARER_MTK_CCCI, MMBearerMtkCcci))
#define MM_BEARER_MTK_CCCI_CLASS(klass)    (G_TYPE_CHECK_CLASS_CAST ((klass),  MM_TYPE_BEARER_MTK_CCCI, MMBearerMtkCcciClass))
#define MM_IS_BEARER_MTK_CCCI(obj)         (G_TYPE_CHECK_INSTANCE_TYPE ((obj), MM_TYPE_BEARER_MTK_CCCI))
#define MM_IS_BEARER_MTK_CCCI_CLASS(klass) (G_TYPE_CHECK_CLASS_TYPE ((klass),  MM_TYPE_BEARER_MTK_CCCI))
#define MM_BEARER_MTK_CCCI_GET_CLASS(obj)  (G_TYPE_INSTANCE_GET_CLASS ((obj),  MM_TYPE_BEARER_MTK_CCCI, MMBearerMtkCcciClass))

typedef struct _MMBearerMtkCcci MMBearerMtkCcci;
typedef struct _MMBearerMtkCcciClass MMBearerMtkCcciClass;
typedef struct _MMBearerMtkCcciPrivate MMBearerMtkCcciPrivate;

struct _MMBearerMtkCcci {
    MMBroadbandBearer parent;
    MMBearerMtkCcciPrivate *priv;
};

struct _MMBearerMtkCcciClass {
    MMBroadbandBearerClass parent;
};

GType mm_bearer_mtk_ccci_get_type (void);

void          mm_bearer_mtk_ccci_new        (MMBroadbandModem    *modem,
                                             MMBearerProperties  *config,
                                             GCancellable        *cancellable,
                                             GAsyncReadyCallback  callback,
                                             gpointer             user_data);
MMBaseBearer *mm_bearer_mtk_ccci_new_finish (GAsyncResult        *res,
                                             GError             **error);

#endif /* MM_BEARER_MTK_CCCI_H */
