/* A neutral test store runtime for the SDK tests: the API tests/fixtures/sdk_binding binds the
 * generated SDKs to, implemented by tests/fixtures/sdk/store_fake.c, a recording store. Built
 * by tests/contract_sdk.rs. */
#ifndef STORE_RUNTIME_H
#define STORE_RUNTIME_H

#include <stdbool.h>
#include <stdint.h>

#define STORE_RUNTIME_MAJOR 1
#define STORE_RUNTIME_MINOR 2
#define STORE_DIGEST_BYTES 32u
#define STORE_MODEL_HASH_CHARS 64u
#define STORE_CMD_ORIGIN_ID_BYTES 27u

extern const uint8_t store_runtime_major_1;

typedef enum {
    STORE_OK = 0, STORE_E_REFUSED = 1, STORE_E_RIGHTS = 2, STORE_E_RATE = 3, STORE_E_BUSY = 4, STORE_E_AGAIN = 5,
    STORE_E_TOOBIG = 6, STORE_E_TYPE = 7, STORE_E_RESERVED = 8, STORE_E_UNSET = 9, STORE_E_DISCONNECTED = 10
} store_status_t;

typedef enum { STORE_TRANSPORT_UNIX = 0, STORE_TRANSPORT_INPROC = 1, STORE_TRANSPORT_UART = 2 } store_transport_t;

typedef struct { uint64_t opaque[64]; } store_client_t;

typedef struct {
    const char *endpoint;
    uint32_t struct_size;
    uint32_t transport;
    uint32_t principal_id;
} store_client_cfg_t;

typedef struct {
    const uint32_t *keyset_id;
    const uint8_t *keyset_type;
    uint64_t token;
    uint32_t keyset_len;
    uint16_t contract_major;
    uint16_t contract_minor;
    uint8_t digest[STORE_DIGEST_BYTES];
    char model_hash[STORE_MODEL_HASH_CHARS];
} store_identity_t;

typedef struct { uint64_t opaque[2]; } store_buf_t;

/* One delivered event; a struct key's value arrives in `buf`. */
typedef struct {
    uint64_t commit_seq;
    store_buf_t buf;
    uint32_t key;
} store_event_t;

typedef struct { uint32_t epoch; uint32_t cmd_seq; } store_cmd_id_t;

typedef struct {
    uint32_t source;
    uint8_t id_len;
    uint8_t id[STORE_CMD_ORIGIN_ID_BYTES];
} store_cmd_origin_t;

store_status_t store_client_init(store_client_t *c, const store_client_cfg_t *cfg);
store_status_t store_connect(store_client_t *c, const store_identity_t *id);
store_status_t store_get(store_client_t *c, uint32_t key, void *buf, uint32_t cap, uint32_t *len, uint64_t *seq);
store_status_t store_set(store_client_t *c, uint32_t key, const void *val, uint32_t len, uint64_t *commit_seq);
store_status_t store_batch_add(store_client_t *c, uint32_t key, const void *val, uint32_t len);
store_status_t store_struct_alloc(store_client_t *c, uint32_t key, uint32_t size, store_buf_t *buf);
store_status_t store_struct_publish(store_client_t *c, store_buf_t *buf, uint64_t *commit_seq);
store_status_t store_struct_submit(store_client_t *c, store_buf_t *buf, const store_cmd_origin_t *origin, uint64_t *commit_seq);
store_status_t store_buf_data(const store_buf_t *buf, void **data, uint32_t *len);
store_status_t store_buf_view(const store_event_t *ev, const void **data, uint32_t *len);
void store_buf_release(store_client_t *c, store_buf_t *buf);
store_status_t store_cmd_submit(store_client_t *c, uint32_t cmd_key, const store_cmd_origin_t *origin, const void *val, uint32_t len, uint64_t *commit_seq);

#endif /* STORE_RUNTIME_H */
