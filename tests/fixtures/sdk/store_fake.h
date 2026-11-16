/* The recording runtime's state, for the SDK checks. */
#ifndef STORE_FAKE_H
#define STORE_FAKE_H

#include "store.h"

#define STORE_FAKE_KEYS 64u

typedef struct {
    int used;
    uint32_t key;
    uint32_t len;
    uint8_t bytes[1024];
} store_fake_value_t;

typedef struct {
    store_fake_value_t values[STORE_FAKE_KEYS];
    store_fake_value_t *last;
    char log[512];
    store_client_cfg_t cfg;
    store_identity_t id;
    store_cmd_origin_t origin;
    uint32_t struct_key;
    uint32_t struct_len;
    uint8_t struct_bytes[65536 + 8];
} store_fake_t;

extern store_fake_t store_fake;

void store_fake_put(uint32_t key, const void *val, uint32_t len);

#endif /* STORE_FAKE_H */
