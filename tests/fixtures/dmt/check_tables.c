/* Links against a generated dmt_tables.c and walks it, the way a store core would: every table
 * the header declares must be defined, sorted and consistent. Built by tests/contract_mode.rs. */
#include <stddef.h>
#include <stdint.h>

#include "dmt_tables.h"

_Static_assert(DMT_TIV_MAJOR == 1, "table interface major");
_Static_assert(DMT_NUM_RESERVED >= 1, "ID 0 is always reserved");

/**
 * @brief   Whether `id` is a key of the tables (binary search, as a core would).
 * @version 1
 * @return  1 when found, 0 otherwise.
 */
static int has_key(uint32_t id)
{
    size_t lo = 0u;
    size_t hi = DMT_NUM_KEYS;
    while (lo < hi) {
        size_t mid = lo + (hi - lo) / 2u;
        if (dmt_keys[mid].id == id) {
            return 1;
        }
        if (dmt_keys[mid].id < id) {
            lo = mid + 1u;
        } else {
            hi = mid;
        }
    }
    return 0;
}

/**
 * @brief   Whether a command's owner is wrong: the core answers RESET_REQUEST and TRACE_ENABLE
 *          (DMT_OWNER_CORE); every other command has a principal index.
 * @version 1
 * @return  1 when wrong, 0 otherwise.
 */
static int owner_wrong(const dmt_command_t *c)
{
    int core = c->cmd_key == dmt_wk[DMT_WK_RESET_REQUEST] || c->cmd_key == dmt_wk[DMT_WK_TRACE_ENABLE];
    return core ? c->owner_principal != DMT_OWNER_CORE : c->owner_principal >= DMT_NUM_PRINCIPALS;
}

/**
 * @brief   Count violations of the table invariants.
 *          v2: every principal is named (not only UNIX token-0 ones).
 * @version 2
 * @return  0 when the tables are consistent.
 */
static int violations(void)
{
    int bad = 0;
    size_t i;
    for (i = 1u; i < DMT_NUM_KEYS; i++) {
        bad += dmt_keys[i - 1u].id >= dmt_keys[i].id;            /* sorted, unique */
    }
    for (i = 0u; i < DMT_NUM_KEYS; i++) {
        bad += dmt_keys[i].id == DMT_ID_NONE;
        bad += ((dmt_keys[i].id >> 3) & 0xFu) != dmt_keys[i].type;  /* type bits */
        bad += dmt_keys[i].default_off + dmt_keys[i].default_len > DMT_DEFAULT_BLOB_LEN;
        bad += (dmt_keys[i].flags & DMT_KF_ALIAS) && !has_key(dmt_keys[i].alias_target);
        bad += dmt_keys[i].type == DMT_T_STRUCT && dmt_keys[i].xport != DMT_X_UNIX;
        bad += (dmt_keys[i].flags & DMT_KF_SECRET) && (dmt_keys[i].xport & DMT_X_UART);
        bad += (dmt_keys[i].flags & DMT_KF_MFG) && !(dmt_keys[i].flags & DMT_KF_SURVIVES_RESET);
        bad += dmt_keys[i].id == dmt_wk[DMT_WK_DEVICE_ID] && !(dmt_keys[i].flags & DMT_KF_MFG);
    }
    bad += dmt_reserved_ids[0] != DMT_ID_NONE;
    for (i = 1u; i < DMT_NUM_RESERVED; i++) {
        bad += dmt_reserved_ids[i - 1u] >= dmt_reserved_ids[i];
        bad += has_key(dmt_reserved_ids[i]);
    }
    for (i = 0u; i < DMT_WK_REQUIRED_COUNT; i++) {
        bad += !has_key(dmt_wk[i]);
    }
    for (i = 0u; i < DMT_NUM_COMMANDS; i++) {
        bad += !has_key(dmt_commands[i].cmd_key) + !has_key(dmt_commands[i].result_key)
            + !has_key(dmt_commands[i].dedupe_key);
        bad += owner_wrong(&dmt_commands[i]);
    }
    for (i = 0u; i < DMT_NUM_PRINCIPALS; i++) {
        bad += dmt_principal_names[i] == NULL || dmt_principal_names[i][0] == '\0';
        bad += dmt_principals[i].heartbeat_key != DMT_ID_NONE
            && !has_key(dmt_principals[i].heartbeat_key);
    }
    return bad;
}

/**
 * @brief   Whether UNIX principal `p` and its uid's totals (when declared) take one value of
 *          `max` bytes in flight.
 * @version 1
 * @return  1 when they do.
 */
static int takes(size_t p, uint32_t max)
{
    const dmt_principal_t *q = &dmt_principals[p];
    const size_t rows = DMT_NUM_UID_CAPS;   /* a variable: 0 rows is no constant comparison */
    int uid = 1;
    size_t i;
    for (i = 0u; i < rows; i++) {
        uid &= dmt_uid_caps[i].uid != q->uid
            || (dmt_uid_caps[i].inflight_fd > 0u && dmt_uid_caps[i].inflight_bytes >= max);
    }
    return q->cap_inflight_fd > 0u && q->cap_inflight_bytes >= max && uid;
}

/**
 * @brief   Count UNIX principals that may read a struct key but cannot take its largest value in
 *          flight (a store refuses such tables at startup).
 * @version 1
 * @return  the number of (principal, key) violations.
 */
static int struct_cap_violations(void)
{
    int bad = 0;
    size_t p, k;
    for (p = 0u; p < DMT_NUM_PRINCIPALS; p++) {
        for (k = 0u; k < DMT_NUM_KEYS && dmt_principals[p].kind == DMT_PK_UNIX; k++) {
            bad += dmt_keys[k].type == DMT_T_STRUCT
                && (DMT_RIGHTS(dmt_rights[p], k) & DMT_RIGHT_READ) != 0u
                && !takes(p, dmt_keys[k].max_size);
        }
    }
    return bad;
}

/**
 * @brief   Count violations of one log stream row: a known principal (the core's stream only
 *          as row 0) and keys, OFF and the boot backing allowed, file levels within RAM levels,
 *          a line within the ring.
 *          v2: the core's row and the state key.
 * @version 2
 * @return  the number of violations.
 */
static int log_row_violations(size_t i, const dmt_log_stream_t *s, const char *name)
{
    int bad = 0;
    const uint8_t off = 1u << DMT_LB_OFF;
    const int core = s->principal == DMT_LOG_PRINCIPAL_CORE;
    bad += (core ? i != 0u : s->principal >= DMT_NUM_PRINCIPALS) || name == NULL || name[0] == '\0';
    bad += !has_key(s->ctl_key) || !has_key(s->drop_counter_key) || !has_key(s->state_key);
    bad += !(s->allowed_backings & off) || !(s->allowed_backings & (1u << s->backing));
    bad += (s->level_file & ~s->level_ram) != 0u || s->reserved0 != 0u || s->max_line_bytes == 0u;
    bad += s->allowed_backings != off && s->max_line_bytes > s->ram_cap_bytes;
    return bad;
}

/**
 * @brief   Count log table violations, and RAM sums above the budgets a store re-checks at
 *          startup: every stream that may be enabled, and every stream enabled at boot.
 *          v2: rows are checked with their index.
 * @version 2
 * @return  the number of violations.
 */
static int log_violations(void)
{
    const size_t rows = DMT_NUM_LOG_STREAMS;   /* a variable: 0 rows is no constant comparison */
    const uint8_t off = 1u << DMT_LB_OFF;
    uint64_t worst = 0u;
    uint64_t boot = 0u;
    int bad = 0;
    size_t i;
    for (i = 0u; i < rows; i++) {
        const dmt_log_stream_t *s = &dmt_log_streams[i];
        bad += log_row_violations(i, s, dmt_log_stream_names[i]);
        worst += s->allowed_backings != off ? s->ram_cap_bytes : 0u;
        boot += s->backing != DMT_LB_OFF ? s->ram_cap_bytes : 0u;
    }
    bad += worst > DMT_LOG_RAM_BUDGET_BYTES;
    bad += DMT_LOG_RAM_DEFAULT_BUDGET_BYTES != 0u && boot > DMT_LOG_RAM_DEFAULT_BUDGET_BYTES;
    return bad;
}

/**
 * @brief   Walk the tables; the link itself proves every declared table is defined.
 *          v2: struct readers take a struct's largest value in flight.
 *          v3: the log streams.
 * @version 3
 * @return  process exit status: 0 when consistent.
 */
int main(void)
{
    const void *const tables[] = { dmt_digest, dmt_rights, dmt_uid_caps, dmt_default_blob,
                                   dmt_log_streams, dmt_log_stream_names };
    size_t i;
    int missing = 0;
    for (i = 0u; i < sizeof tables / sizeof tables[0]; i++) {
        missing += tables[i] == NULL;
    }
    missing += struct_cap_violations();
    missing += log_violations();
    return (violations() == 0 && missing == 0 && dmt_tiv_major_1 == 1u) ? 0 : 1;
}
