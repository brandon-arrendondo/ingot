/* Pins the generated tables of examples/contract (variant sku_a) field by field, with values
 * derived by hand from the model: every column of dmt_keys, dmt_principals, dmt_commands and
 * dmt_uid_caps, the defaults blob, reserved IDs, well-known roles, rights cells and the header
 * defines. Built by tests/contract_mode.rs against the 1.0, 1.1 and 2.0 outputs. */
#include <stddef.h>
#include <stdint.h>
#include <string.h>

#include "dmt_tables.h"

static int bad;

/* drive.command.map_edit's burst on its command, result and dedupe keys: the default at 1.0,
 * its declared rate_burst from 1.1 on. */
#if DMT_CONTRACT_MAJOR == 1 && DMT_CONTRACT_MINOR == 0
#define MAP_EDIT_BURST 4u
#else
#define MAP_EDIT_BURST 8u
#endif

#define EXPECT(cond) do { if (!(cond)) { bad++; } } while (0)

/**
 * @brief   Index of key `id` in dmt_keys, or DMT_NUM_KEYS when absent.
 * @version 1
 * @return  the index
 */
static size_t key_at(uint32_t id)
{
    size_t i;
    for (i = 0u; i < DMT_NUM_KEYS && dmt_keys[i].id != id; i++) {
    }
    return i;
}

/**
 * @brief   Check every column of key `id`.
 * @version 1
 */
static void key(uint32_t id, uint32_t max, uint32_t rate, uint32_t burst, uint32_t deferred,
                uint16_t flags, uint16_t minor, uint8_t type, uint8_t persist, uint8_t role,
                uint8_t xport)
{
    size_t i = key_at(id);
    EXPECT(i < DMT_NUM_KEYS);
    if (i < DMT_NUM_KEYS) {
        const dmt_key_t *k = &dmt_keys[i];
        EXPECT(k->max_size == max && k->rate_per_day == rate && k->rate_burst == burst);
        EXPECT(k->deferred_ms == deferred && k->flags == flags && k->introduced_in == minor);
        EXPECT(k->type == type && k->persist == persist && k->role == role && k->xport == xport);
    }
}

/**
 * @brief   Check key `id`'s default bytes (and that it has no alias unless `alias` is set).
 * @version 1
 */
static void value(uint32_t id, const void *bytes, uint32_t len, uint32_t alias)
{
    size_t i = key_at(id);
    EXPECT(i < DMT_NUM_KEYS);
    if (i < DMT_NUM_KEYS) {
        const dmt_key_t *k = &dmt_keys[i];
        EXPECT(k->default_len == len && k->alias_target == alias);
        EXPECT(len == 0u || memcmp(&dmt_default_blob[k->default_off], bytes, len) == 0);
    }
}

/**
 * @brief   Rights bits of principal `p` on key `id`.
 * @version 1
 * @return  the DMT_RIGHT_* bits
 */
static unsigned rights(size_t p, uint32_t id)
{
    return DMT_RIGHTS(dmt_rights[p], key_at(id));
}

/**
 * @brief   Check one principal row and its declared name.
 *          v2: every principal has a name (sub-consumers, UART and in-process ones too).
 * @version 2
 */
static void principal(size_t p, uint32_t uid, uint16_t token, uint8_t kind, uint8_t flags,
                      uint32_t hb, uint32_t window, uint32_t latency, const char *name)
{
    const dmt_principal_t *q = &dmt_principals[p];
    EXPECT(q->uid == uid && q->token == token && q->kind == kind && q->flags == flags);
    EXPECT(q->heartbeat_key == hb && q->heartbeat_window_ms == window);
    EXPECT(q->latency_budget_ms == latency && q->queue_bound == 16u && q->reserved0 == 0u);
    EXPECT(dmt_principal_names[p] != NULL && strcmp(dmt_principal_names[p], name) == 0);
}

/**
 * @brief   Check one command row.
 * @version 1
 */
static void command(size_t c, const uint32_t keys[3], uint32_t payload, uint32_t result, uint16_t edits,
                    uint16_t ring, uint16_t owner, uint8_t persist)
{
    const dmt_command_t *r = &dmt_commands[c];
    EXPECT(r->cmd_key == keys[0] && r->result_key == keys[1] && r->dedupe_key == keys[2]);
    EXPECT(r->max_payload_bytes == payload && r->max_result_bytes == result);
    EXPECT(r->max_edits == edits && r->dedupe_ring == ring && r->owner_principal == owner);
    EXPECT(r->persist == persist && r->reserved0 == 0u);
}

/**
 * @brief   Values every version of the generic model shares.
 *          v2: the enum key drive.settings.mode (KF_ENUM) and the contract patch.
 *          v3: the keys the shadow mapping of 1.x uses (fan, its twins, tones, docked), the
 *              drive command and its argument keys.
 *          v4: a sub-consumer's, the in-process and the UART principal's names.
 *          v5: drive.map.map_data moved to versioned (2.0 raises its max_size).
 *          v6: drive.command.map_edit's keys at MAP_EDIT_BURST.
 * @version 6
 */
static void common(void)
{
    static const uint8_t mode[] = { 1 };
    static const uint8_t fan[] = { 1 };
    key(0x00C00288u, 1u, 1440u, 4u, 0u, 0x0008u, 0u, DMT_T_U8, DMT_P_DURABLE, DMT_R_SETTINGS, 0x07u);
    value(0x00C00288u, fan, sizeof fan, 0u);
    key(0x00C20388u, 1u, 0u, 0u, 0u, 0x0008u, 0u, DMT_T_U8, DMT_P_NONE, DMT_R_STATE, 0x07u);
    key(0x00C20410u, 2u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_U16, DMT_P_NONE, DMT_R_STATE, 0x07u);
    key(0x00C20540u, 8u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_STRING, DMT_P_NONE, DMT_R_STATE, 0x07u);
    key(0x00C20580u, 1u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_BOOL, DMT_P_NONE, DMT_R_STATE, 0x07u);
    key(0x00C601C8u, 72u, 0u, 0u, 0u, 0x0010u, 0u, DMT_T_BINARY, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    key(0x00C60248u, 28u, 0u, 0u, 0u, 0x0020u, 0u, DMT_T_BINARY, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    key(0x00C602C8u, 256u, 0u, 0u, 0u, 0x0040u, 0u, DMT_T_BINARY, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    key(0x00C60318u, 4u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_U32, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    key(0x00C60390u, 2u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_U16, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    key(0x00C60410u, 2u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_U16, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    key(0x00C604C0u, 512u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_STRING, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    static const uint8_t serial[] = { 'u', 'n', 's', 'e', 't' };
    EXPECT(strcmp(DMT_CONTRACT_NAME, "generic") == 0 && strcmp(DMT_VARIANT, "sku_a") == 0);
    EXPECT(DMT_CONTRACT_PATCH == 0);
    EXPECT(DMT_STAGING_TOTAL_BYTES == 262144u && DMT_INPROC_QUEUE_BYTES == 8192u);
    EXPECT(DMT_INPROC_SNAPSHOT_BYTES == 4096u && DMT_NUM_UID_CAPS == 3);
    key(0x00400000u, 1u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_BOOL, DMT_P_NONE, DMT_R_DIAG, 0x07u);
    key(0x00400098u, 4u, 24u, 4u, 30000u, 0x0000u, 0u, DMT_T_U32, DMT_P_DEFERRED, DMT_R_DIAG, 0x07u);
    key(0x004002C8u, 512u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_BINARY, DMT_P_NONE, DMT_R_DIAG, 0x07u);
    key(0x00420018u, 4u, 24u, 4u, 0u, 0x0002u, 0u, DMT_T_U32, DMT_P_DURABLE, DMT_R_CONFIG, 0x07u);
    key(0x00440048u, 8u, 1u, 4u, 0u, 0x0010u, 0u, DMT_T_BINARY, DMT_P_DURABLE, DMT_R_COMMAND, 0x07u);
    key(0x004400C8u, 12u, 1u, 4u, 0u, 0x0020u, 0u, DMT_T_BINARY, DMT_P_DURABLE, DMT_R_COMMAND, 0x07u);
    key(0x00440148u, 256u, 1u, 4u, 0u, 0x0040u, 0u, DMT_T_BINARY, DMT_P_DURABLE, DMT_R_COMMAND, 0x07u);
    key(0x00440348u, 126u, 0u, 0u, 0u, 0x0010u, 0u, DMT_T_BINARY, DMT_P_NONE, DMT_R_COMMAND, 0x07u);
    key(0x00800040u, 64u, 1u, 4u, 0u, 0x0082u, 0u, DMT_T_STRING, DMT_P_DURABLE, DMT_R_CONFIG, 0x07u);
    key(0x00C20098u, 4u, 86400u, 4u, 5000u, 0x0000u, 0u, DMT_T_U32, DMT_P_DEFERRED, DMT_R_STATE, 0x07u);
    key(0x00C60050u, 32776u, 1440u, MAP_EDIT_BURST, 0u, 0x0010u, 0u, DMT_T_STRUCT, DMT_P_DURABLE, DMT_R_COMMAND, 0x01u);
    key(0x00C600C8u, 268u, 1440u, MAP_EDIT_BURST, 0u, 0x0020u, 0u, DMT_T_BINARY, DMT_P_DURABLE, DMT_R_COMMAND, 0x07u);
    key(0x00C60148u, 512u, 1440u, MAP_EDIT_BURST, 0u, 0x0040u, 0u, DMT_T_BINARY, DMT_P_DURABLE, DMT_R_COMMAND, 0x07u);
    key(0x01000040u, 64u, 1u, 4u, 0u, 0x0001u, 0u, DMT_T_STRING, DMT_P_DURABLE, DMT_R_CONFIG, 0x03u);
    key(0x00C00088u, 1u, 1440u, 4u, 0u, 0x0008u, 0u, DMT_T_U8, DMT_P_DURABLE, DMT_R_SETTINGS, 0x07u);
    value(0x008000C0u, serial, sizeof serial, 0u);
    value(0x00C00088u, mode, sizeof mode, 0u);
    value(0x00400098u, NULL, 0u, 0u);
    principal(0u, 2001u, 0u, DMT_PK_UNIX, 0x01u, 0x01020018u, 2000u, 50u, "integrator");
    principal(1u, 2002u, 0u, DMT_PK_UNIX, 0x04u, 0x01020098u, 5000u, 100u, "app");
    principal(2u, 2002u, 1u, DMT_PK_UNIX, 0x00u, 0u, 0u, 100u, "app.cloud");
    principal(DMT_NUM_PRINCIPALS - 2u, 10u, 0u, DMT_PK_INPROC, 0x00u, 0u, 0u, 50u, "inproc");
    principal(DMT_NUM_PRINCIPALS - 1u, 1u, 0u, DMT_PK_UART, 0x00u, 0u, 0u, 20u, "mcu");
    EXPECT(dmt_principals[0].cap_inflight_fd == 4u && dmt_principals[0].cap_inflight_bytes == 1048576u);
    EXPECT(dmt_principals[0].cap_open_fds == 16u && dmt_principals[0].cap_trace_share_pct == 25u);
    EXPECT(dmt_principals[0].resync_k == 3u && dmt_principals[0].resync_t_ms == 60000u);
    EXPECT(dmt_principals[0].staging_bound_bytes == 65536u && dmt_principals[0].introduced_in == 0u);
    EXPECT(dmt_uid_caps[0].uid == 2001u && dmt_uid_caps[1].uid == 2002u && dmt_uid_caps[2].uid == 2003u);
    EXPECT(dmt_uid_caps[1].inflight_fd == 16u && dmt_uid_caps[1].inflight_bytes == 8388608u);
    EXPECT(dmt_uid_caps[1].open_fds == 48u);
    static const uint32_t reset[3] = { 0x00440048u, 0x004400C8u, 0x00440148u };
    static const uint32_t pre_reset[3] = { 0x004401C8u, 0x00440248u, 0x004402C8u };
    static const uint32_t trace[3] = { 0x00440348u, 0x004403C8u, 0x00440448u };
    static const uint32_t map_edit[3] = { 0x00C60050u, 0x00C600C8u, 0x00C60148u };
    static const uint32_t drive[3] = { 0x00C601C8u, 0x00C60248u, 0x00C602C8u };
    command(0u, reset, 0u, 0u, 0u, 4u, DMT_OWNER_CORE, DMT_P_DURABLE);
    command(1u, pre_reset, 0u, 0u, 0u, 4u, 0u, DMT_P_DURABLE);
    command(2u, trace, 118u, 0u, 0u, 4u, DMT_OWNER_CORE, DMT_P_NONE);
    command(3u, map_edit, 32768u, 256u, 32u, 8u, 4u, DMT_P_DURABLE);
    command(4u, drive, 64u, 16u, 0u, 4u, 0u, DMT_P_NONE);
    EXPECT(dmt_wk[DMT_WK_RESET_REQUEST] == 0x00440048u && dmt_wk[DMT_WK_TRACE_ENABLE] == 0x00440348u);
    EXPECT(dmt_wk[DMT_WK_RESET_PRE_COMMAND] == 0x004401C8u && dmt_wk[DMT_WK_DEVICE_ID] == 0x00800040u);
    EXPECT(dmt_wk[DMT_WK_DIAG_FSYNC_COUNT] == 0x00400218u && dmt_wk[DMT_WK_DIAG_BYTES_WRITTEN_KIB] == 0u);
    EXPECT(rights(0u, 0x01000040u) == 0u && rights(3u, 0x01000040u) == 3u);
    EXPECT(rights(0u, 0x01020018u) == 3u && rights(1u, 0x01020018u) == 0u);
    EXPECT(rights(0u, 0x010000C8u) == 1u);   /* integrator reads cloud_token (reader) */
}

/**
 * @brief   Values of one version: 1.0 has the speed key and gain; 1.1 adds velocity (speed is
 *          its alias) and app.ota; 2.0 drops speed, label and gain and adds gain_pct.
 *          v2: drive.map.map_data: 64 KiB in 1.x, 128 KiB in 2.0.
 *          v3: 1.1 adds the four log keys.
 * @version 3
 */
static void versioned(void)
{
    static const uint8_t limit[] = { 100, 0 };
    value(0x00C00010u, limit, sizeof limit, 0u);
#if DMT_CONTRACT_MAJOR == 1
    static const uint8_t gain[] = { 0x00, 0x00, 0xC0, 0x3F };   /* 1.5f */
    static const uint8_t label[] = { 'i', 'd', 'l', 'e' };
    key(0x00C40050u, 65536u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_STRUCT, DMT_P_NONE, DMT_R_STATE, 0x01u);
    EXPECT(DMT_NUM_RESERVED == 2 && dmt_reserved_ids[1] == 0x00C20480u);
    value(0x00C00138u, gain, sizeof gain, 0u);
    value(0x00C201C0u, label, sizeof label, 0u);
#if DMT_CONTRACT_MINOR == 0
    EXPECT(DMT_NUM_KEYS == 48 && DMT_NUM_PRINCIPALS == 8 && DMT_DEFAULT_BLOB_LEN == 18);
    key(0x00C20028u, 2u, 864000u, 20u, 0u, 0x0000u, 0u, DMT_T_I16, DMT_P_NONE, DMT_R_STATE, 0x07u);
#else
    EXPECT(DMT_NUM_KEYS == 54 && DMT_NUM_PRINCIPALS == 9);
    key(0x00C20028u, 2u, 864000u, 20u, 0u, 0x0004u, 0u, DMT_T_I16, DMT_P_NONE, DMT_R_STATE, 0x07u);
    value(0x00C20028u, NULL, 0u, 0x00C202A8u);
    key(0x00C202A8u, 2u, 864000u, 20u, 0u, 0x0000u, 1u, DMT_T_I16, DMT_P_NONE, DMT_R_STATE, 0x07u);
    EXPECT(dmt_principals[5].token == 4u && dmt_principals[5].introduced_in == 1u);
#endif
#else
    static const uint8_t gain_pct[] = { 150 };
    EXPECT(DMT_NUM_RESERVED == 5 && key_at(0x00C20028u) == DMT_NUM_KEYS);
    key(0x00C40050u, 131072u, 0u, 0u, 0u, 0x0000u, 0u, DMT_T_STRUCT, DMT_P_NONE, DMT_R_STATE, 0x01u);
    key(0x00C202A8u, 2u, 864000u, 20u, 0u, 0x0000u, 0u, DMT_T_I16, DMT_P_NONE, DMT_R_STATE, 0x07u);
    value(0x00C00208u, gain_pct, sizeof gain_pct, 0u);
    EXPECT(dmt_principals[5].introduced_in == 0u);
#endif
}

/**
 * @brief   Index of the first principal of `kind`, or DMT_NUM_PRINCIPALS when there is none.
 * @version 1
 * @return  the index
 */
static size_t principal_of_kind(uint8_t kind)
{
    size_t p;
    for (p = 0u; p < DMT_NUM_PRINCIPALS && dmt_principals[p].kind != kind; p++) {
    }
    return p;
}

/**
 * @brief   The combinations a store batch and an in-process client need, in every version:
 *          the integrator writes a durable key (trip_count), deferred keys at two intervals
 *          (odometer 5000 ms, run_time 60000 ms) and volatile keys of drive.state; the in-process
 *          role writes drive.settings.mode (durable), submits the drive command and reads the rest.
 * @version 1
 */
static void batches(void)
{
    size_t inproc = principal_of_kind(DMT_PK_INPROC);
    key(0x00C20618u, 4u, 1440u, 4u, 0u, 0x0000u, 0u, DMT_T_U32, DMT_P_DURABLE, DMT_R_STATE, 0x07u);
    key(0x00C20698u, 4u, 86400u, 4u, 60000u, 0x0000u, 0u, DMT_T_U32, DMT_P_DEFERRED, DMT_R_STATE, 0x07u);
    EXPECT(rights(0u, 0x00C20618u) == 3u && rights(0u, 0x00C20698u) == 3u);
    EXPECT(rights(0u, 0x00C20098u) == 3u && rights(0u, 0x00C20100u) == 3u);
    EXPECT(inproc < DMT_NUM_PRINCIPALS && dmt_principals[inproc].uid == 10u);
    EXPECT(rights(inproc, 0x00C00088u) == 3u && rights(inproc, 0x00C601C8u) == 2u);
    EXPECT(rights(inproc, 0x00C20618u) == 1u && rights(inproc, 0x00C60248u) == 0u);
}

#if !(DMT_CONTRACT_MAJOR == 1 && DMT_CONTRACT_MINOR == 0)
/**
 * @brief   Check one log stream row against its hand-derived values (flash figures, flush mask
 *          and introduced_in are checked by the caller).
 * @version 1
 */
static void log_stream(size_t i, uint32_t ram, uint32_t ctl, uint32_t drop, uint16_t principal,
                       uint8_t backing, uint8_t allowed, uint8_t level_ram, uint8_t level_file,
                       const char *name)
{
    const dmt_log_stream_t *s = &dmt_log_streams[i];
    EXPECT(s->ram_cap_bytes == ram && s->max_line_bytes == 256u);
    EXPECT(s->ctl_key == ctl && s->drop_counter_key == drop && s->principal == principal);
    EXPECT(s->backing == backing && s->allowed_backings == allowed);
    EXPECT(s->level_ram == level_ram && s->level_file == level_file && s->reserved0 == 0u);
    EXPECT(strcmp(dmt_log_stream_names[i], name) == 0);
}
#endif

/**
 * @brief   The log streams: none in 1.0; from 1.1 on integrator's (a ring at boot, flash on
 *          trigger allowed on sku_a) and app.cloud's (off at boot, a ring allowed), the budgets,
 *          and the switch and drop-counter keys (core.log.*_cfg written by integrator, index 0).
 * @version 1
 */
static void logs(void)
{
#if DMT_CONTRACT_MAJOR == 1 && DMT_CONTRACT_MINOR == 0
    EXPECT(DMT_NUM_LOG_STREAMS == 0 && DMT_LOG_RAM_BUDGET_BYTES == 0u && DMT_LOG_FLASH_BYTES == 0u);
#else
    const uint16_t minor = DMT_CONTRACT_MAJOR == 1 ? 1u : 0u;
    const dmt_log_stream_t *s = &dmt_log_streams[0];
    EXPECT(DMT_NUM_LOG_STREAMS == 2 && DMT_LOG_RAM_BUDGET_BYTES == 12288u);
    EXPECT(DMT_LOG_RAM_DEFAULT_BUDGET_BYTES == 8192u && DMT_LOG_FLASH_BYTES == 65536u);
    EXPECT(DMT_LOG_FLASH_BYTES_PER_DAY == 262144u);
    log_stream(0u, 8192u, 0x00460018u, 0x00400318u, 0u, DMT_LB_RING,
               (1u << DMT_LB_OFF) | (1u << DMT_LB_RING) | (1u << DMT_LB_FLASH_ON_TRIGGER),
               0x07u, 0x03u, "integrator");
    EXPECT(s->flash_seg_bytes == 16384u && s->flash_seg_count == 4u);
    EXPECT(s->flash_rate_bytes_per_day == 262144u && s->flash_rate_burst_bytes == 16384u);
    EXPECT(s->flush == (DMT_LT_FAULT | DMT_LT_WATCHDOG | DMT_LT_SHUTDOWN | DMT_LT_ERROR_LINE));
    EXPECT(s->introduced_in == minor && dmt_log_streams[1].introduced_in == minor);
    log_stream(1u, 4096u, 0x00460098u, 0x00400398u, 2u, DMT_LB_OFF,
               (1u << DMT_LB_OFF) | (1u << DMT_LB_RING), 0x0Fu, 0x00u, "app_cloud");
    EXPECT(dmt_log_streams[1].flash_seg_bytes == 0u && dmt_log_streams[1].flush == 0u);
    key(0x00400318u, 4u, 0u, 0u, 0u, 0x0000u, minor, DMT_T_U32, DMT_P_NONE, DMT_R_DIAG, 0x07u);
    key(0x00460018u, 4u, 1440u, 4u, 0u, 0x0000u, minor, DMT_T_U32, DMT_P_NONE, DMT_R_CONFIG, 0x07u);
    EXPECT(rights(0u, 0x00460018u) == 3u && rights(0u, 0x00460098u) == 3u);
#endif
}

/**
 * @brief   Run every check.
 *          v3: the log streams.
 * @version 3
 * @return  0 when every value matches.
 */
int main(void)
{
    common();
    versioned();
    batches();
    logs();
    return bad == 0 ? 0 : 1;
}
