/* A recording runtime for the SDK tests: one value per key in memory, every call appended to a
 * log, the last command and struct write kept for inspection. Built by tests/contract_sdk.rs. */
#include <string.h>

#include "store_fake.h"

const uint8_t store_runtime_major_1 = 1u;

store_fake_t store_fake;

/**
 * @brief   Append one call name to the log.
 * @version 1
 */
static void note(const char *op)
{
    strncat(store_fake.log, op, sizeof store_fake.log - strlen(store_fake.log) - 2u);
    strncat(store_fake.log, " ", sizeof store_fake.log - strlen(store_fake.log) - 1u);
}

/**
 * @brief   The slot of `key`, claiming a free one.
 * @version 1
 */
static store_fake_value_t *slot(uint32_t key)
{
    store_fake_value_t *free_slot = NULL;
    for (unsigned i = 0u; i < STORE_FAKE_KEYS; i++) {
        if (store_fake.values[i].used && store_fake.values[i].key == key) {
            return &store_fake.values[i];
        }
        if (!store_fake.values[i].used && free_slot == NULL) {
            free_slot = &store_fake.values[i];
        }
    }
    free_slot->used = 1;
    free_slot->key = key;
    return free_slot;
}

/** @brief Store a value. @version 1 */
void store_fake_put(uint32_t key, const void *val, uint32_t len)
{
    store_fake_value_t *v = slot(key);
    memcpy(v->bytes, val, len);
    v->len = len;
    store_fake.last = v;
}

/** @brief Fake client init: record the configuration. @version 1 */
store_status_t store_client_init(store_client_t *c, const store_client_cfg_t *cfg)
{
    (void)c;
    store_fake.cfg = *cfg;
    note("init");
    return STORE_OK;
}

/** @brief Fake connect: record the identity. @version 1 */
store_status_t store_connect(store_client_t *c, const store_identity_t *id)
{
    (void)c;
    store_fake.id = *id;
    note("connect");
    return STORE_OK;
}

/** @brief Fake get. @version 1 */
store_status_t store_get(store_client_t *c, uint32_t key, void *buf, uint32_t cap, uint32_t *len, uint64_t *seq)
{
    store_fake_value_t *v = slot(key);
    (void)c;
    note("get");
    *seq = 7u;
    *len = v->len;
    if (v->len > cap) {
        return STORE_E_TOOBIG;
    }
    memcpy(buf, v->bytes, v->len);
    return STORE_OK;
}

/** @brief Fake set. @version 1 */
store_status_t store_set(store_client_t *c, uint32_t key, const void *val, uint32_t len, uint64_t *commit_seq)
{
    (void)c;
    note("set");
    store_fake_put(key, val, len);
    *commit_seq = 9u;
    return STORE_OK;
}

/** @brief Fake batch add. @version 1 */
store_status_t store_batch_add(store_client_t *c, uint32_t key, const void *val, uint32_t len)
{
    (void)c;
    note("batch_add");
    store_fake_put(key, val, len);
    return STORE_OK;
}

/** @brief Fake struct alloc. @version 1 */
store_status_t store_struct_alloc(store_client_t *c, uint32_t key, uint32_t size, store_buf_t *buf)
{
    (void)c;
    note("alloc");
    store_fake.struct_key = key;
    store_fake.struct_len = size;
    buf->opaque[0] = 1u;
    return STORE_OK;
}

/** @brief Fake buffer data. @version 1 */
store_status_t store_buf_data(const store_buf_t *buf, void **data, uint32_t *len)
{
    note("data");
    *data = store_fake.struct_bytes;
    *len = store_fake.struct_len;
    return buf->opaque[0] == 1u ? STORE_OK : STORE_E_TYPE;
}

/** @brief Fake event view: the struct bytes when the event carries a buffer, else STORE_E_TYPE. @version 1 */
store_status_t store_buf_view(const store_event_t *ev, const void **data, uint32_t *len)
{
    note("view");
    if (ev->buf.opaque[0] != 1u) {
        return STORE_E_TYPE;
    }
    *data = store_fake.struct_bytes;
    *len = store_fake.struct_len;
    return STORE_OK;
}

/** @brief Fake struct publish. @version 1 */
store_status_t store_struct_publish(store_client_t *c, store_buf_t *buf, uint64_t *commit_seq)
{
    (void)c;
    note("publish");
    buf->opaque[0] = 0u;
    *commit_seq = 11u;
    return STORE_OK;
}

/** @brief Fake struct submit. @version 1 */
store_status_t store_struct_submit(store_client_t *c, store_buf_t *buf, const store_cmd_origin_t *origin, uint64_t *commit_seq)
{
    (void)c;
    note("struct_submit");
    store_fake.origin = *origin;
    buf->opaque[0] = 0u;
    *commit_seq = 12u;
    return STORE_OK;
}

/** @brief Fake buffer release. @version 1 */
void store_buf_release(store_client_t *c, store_buf_t *buf)
{
    (void)c;
    note("release");
    buf->opaque[0] = 0u;
}

/** @brief Fake command submit. @version 1 */
store_status_t store_cmd_submit(store_client_t *c, uint32_t cmd_key, const store_cmd_origin_t *origin, const void *val, uint32_t len, uint64_t *commit_seq)
{
    (void)c;
    note("cmd_submit");
    store_fake.origin = *origin;
    store_fake_put(cmd_key, val, len);
    *commit_seq = 13u;
    return STORE_OK;
}
