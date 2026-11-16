/* Pins the TIV 1.1 content of the generated dmt_types.h: every value, struct size and
 * field offset the table interface fixes. Compiled as C11 by
 * tests/table_types.rs; a mismatch is a compile error. main() exercises DMT_RIGHTS. */
#include <stddef.h>
#include <stdint.h>

#include "dmt_types.h"

#define PIN(expr) _Static_assert(expr, #expr)

PIN(DMT_TIV_MAJOR == 1);
PIN(DMT_TIV_MINOR == 1);

PIN(DMT_T_BOOL == 0); PIN(DMT_T_U8 == 1); PIN(DMT_T_U16 == 2); PIN(DMT_T_U32 == 3);
PIN(DMT_T_I8 == 4); PIN(DMT_T_I16 == 5); PIN(DMT_T_I32 == 6); PIN(DMT_T_F32 == 7);
PIN(DMT_T_STRING == 8); PIN(DMT_T_BINARY == 9); PIN(DMT_T_STRUCT == 10);

PIN(DMT_P_NONE == 0); PIN(DMT_P_DEFERRED == 1); PIN(DMT_P_DURABLE == 2);
PIN(DMT_R_SETTINGS == 0); PIN(DMT_R_STATE == 1); PIN(DMT_R_COMMAND == 2);
PIN(DMT_R_CONFIG == 3); PIN(DMT_R_DIAG == 4);
PIN(DMT_PK_UNIX == 0); PIN(DMT_PK_INPROC == 1); PIN(DMT_PK_UART == 2);

PIN(DMT_KF_SECRET == 0x0001u); PIN(DMT_KF_SURVIVES_RESET == 0x0002u);
PIN(DMT_KF_ALIAS == 0x0004u); PIN(DMT_KF_ENUM == 0x0008u); PIN(DMT_KF_CMD == 0x0010u);
PIN(DMT_KF_CMD_RESULT == 0x0020u); PIN(DMT_KF_CMD_DEDUPE == 0x0040u);
PIN(DMT_KF_MFG == 0x0080u);
PIN(DMT_X_UNIX == 0x01u); PIN(DMT_X_INPROC == 0x02u); PIN(DMT_X_UART == 0x04u);
PIN(DMT_PF_WATCHDOG_REQUIRED == 0x01u); PIN(DMT_PF_TRACE_READER == 0x02u);
PIN(DMT_PF_TOKEN_ISSUER == 0x04u);

PIN(DMT_ID_NONE == 0u); PIN(DMT_DIGEST_BYTES == 32u); PIN(DMT_CMD_HEADER_BYTES == 8u);
PIN(DMT_RESULT_HEADER_BYTES == 12u); PIN(DMT_DEDUPE_ENTRY_BYTES == 64u);
PIN(DMT_OWNER_CORE == 0xFFFFu);
PIN(DMT_RIGHT_READ == 0x1u); PIN(DMT_RIGHT_WRITE == 0x2u);

PIN(sizeof(dmt_key_t) == 40);
PIN(offsetof(dmt_key_t, id) == 0); PIN(offsetof(dmt_key_t, max_size) == 4);
PIN(offsetof(dmt_key_t, rate_per_day) == 8); PIN(offsetof(dmt_key_t, rate_burst) == 12);
PIN(offsetof(dmt_key_t, deferred_ms) == 16); PIN(offsetof(dmt_key_t, alias_target) == 20);
PIN(offsetof(dmt_key_t, default_off) == 24); PIN(offsetof(dmt_key_t, default_len) == 28);
PIN(offsetof(dmt_key_t, flags) == 32); PIN(offsetof(dmt_key_t, introduced_in) == 34);
PIN(offsetof(dmt_key_t, type) == 36); PIN(offsetof(dmt_key_t, persist) == 37);
PIN(offsetof(dmt_key_t, role) == 38); PIN(offsetof(dmt_key_t, xport) == 39);

PIN(sizeof(dmt_principal_t) == 52);
PIN(offsetof(dmt_principal_t, uid) == 0); PIN(offsetof(dmt_principal_t, token) == 4);
PIN(offsetof(dmt_principal_t, kind) == 6); PIN(offsetof(dmt_principal_t, flags) == 7);
PIN(offsetof(dmt_principal_t, heartbeat_key) == 8);
PIN(offsetof(dmt_principal_t, heartbeat_window_ms) == 12);
PIN(offsetof(dmt_principal_t, queue_bound) == 16);
PIN(offsetof(dmt_principal_t, latency_budget_ms) == 20);
PIN(offsetof(dmt_principal_t, cap_inflight_fd) == 24);
PIN(offsetof(dmt_principal_t, cap_inflight_bytes) == 28);
PIN(offsetof(dmt_principal_t, cap_open_fds) == 32);
PIN(offsetof(dmt_principal_t, cap_trace_share_pct) == 36);
PIN(offsetof(dmt_principal_t, resync_k) == 38);
PIN(offsetof(dmt_principal_t, resync_t_ms) == 40);
PIN(offsetof(dmt_principal_t, staging_bound_bytes) == 44);
PIN(offsetof(dmt_principal_t, introduced_in) == 48);
PIN(offsetof(dmt_principal_t, reserved0) == 50);

PIN(sizeof(dmt_uid_caps_t) == 16);
PIN(offsetof(dmt_uid_caps_t, uid) == 0); PIN(offsetof(dmt_uid_caps_t, inflight_fd) == 4);
PIN(offsetof(dmt_uid_caps_t, inflight_bytes) == 8);
PIN(offsetof(dmt_uid_caps_t, open_fds) == 12);

PIN(sizeof(dmt_command_t) == 28);
PIN(offsetof(dmt_command_t, cmd_key) == 0); PIN(offsetof(dmt_command_t, result_key) == 4);
PIN(offsetof(dmt_command_t, dedupe_key) == 8);
PIN(offsetof(dmt_command_t, max_payload_bytes) == 12);
PIN(offsetof(dmt_command_t, max_result_bytes) == 16);
PIN(offsetof(dmt_command_t, max_edits) == 20); PIN(offsetof(dmt_command_t, dedupe_ring) == 22);
PIN(offsetof(dmt_command_t, owner_principal) == 24);
PIN(offsetof(dmt_command_t, persist) == 26); PIN(offsetof(dmt_command_t, reserved0) == 27);

PIN(DMT_WK_RESET_REQUEST == 0); PIN(DMT_WK_RESET_PRE_COMMAND == 1);
PIN(DMT_WK_TRACE_ENABLE == 2); PIN(DMT_WK_SECRETS_FALLBACK == 3); PIN(DMT_WK_BOOT_EPOCH == 4);
PIN(DMT_WK_DIAG_CORRUPTION == 5); PIN(DMT_WK_DIAG_KEYSTORE_VERSION == 6);
PIN(DMT_WK_DIAG_CONTRACT_VERSION == 7); PIN(DMT_WK_DEVICE_ID == 8);
PIN(DMT_WK_DIAG_BYTES_WRITTEN_KIB == 9); PIN(DMT_WK_DIAG_RECORDS_WRITTEN == 10);
PIN(DMT_WK_DIAG_FSYNC_COUNT == 11); PIN(DMT_WK_DIAG_FSYNC_LATENCY_US == 12);
PIN(DMT_WK_DIAG_COMPACTIONS == 13); PIN(DMT_WK_DIAG_JOURNAL_BYTES == 14);
PIN(DMT_WK_DIAG_FLASH_LIFE == 15); PIN(DMT_WK_DIAG_OVERFLOW_RESYNCS == 16);
PIN(DMT_WK_DIAG_HANDSHAKE_REFUSALS == 17); PIN(DMT_WK_DIAG_RIGHTS_REJECTIONS == 18);
PIN(DMT_WK_DIAG_RATE_REJECTIONS == 19); PIN(DMT_WK_DIAG_REPLAY_SKIPPED == 20);
PIN(DMT_WK_DIAG_TORN_TAILS == 21); PIN(DMT_WK_DIAG_ALLOC_GAPS == 22);
PIN(DMT_WK_DIAG_CONNECTED == 23); PIN(DMT_WK_DIAG_SUBSCRIBERS == 24);
PIN(DMT_WK_DIAG_TRACE_REFUSALS == 25); PIN(DMT_WK_COUNT == 26);
PIN(DMT_WK_REQUIRED_COUNT == 9u);

/* TIV 1.1: log streams. */
PIN(DMT_LB_OFF == 0); PIN(DMT_LB_RING == 1); PIN(DMT_LB_FLASH_ON_TRIGGER == 2);
PIN(DMT_LB_FLASH_GROUP == 3);
PIN(DMT_LL_ERROR == 0); PIN(DMT_LL_WARNING == 1); PIN(DMT_LL_INFO == 2); PIN(DMT_LL_DEBUG == 3);
PIN(DMT_LT_FAULT == 0x0001u); PIN(DMT_LT_WATCHDOG == 0x0002u); PIN(DMT_LT_SHUTDOWN == 0x0004u);
PIN(DMT_LT_REBOOT == 0x0008u); PIN(DMT_LT_ERROR_LINE == 0x0010u); PIN(DMT_LT_REQUEST == 0x0020u);
PIN(DMT_LT_RESTART_STORM == 0x0040u); PIN(DMT_LT_HIGH_WATER == 0x0080u);

PIN(sizeof(dmt_log_stream_t) == 44);
PIN(offsetof(dmt_log_stream_t, ram_cap_bytes) == 0);
PIN(offsetof(dmt_log_stream_t, max_line_bytes) == 4);
PIN(offsetof(dmt_log_stream_t, flash_seg_bytes) == 8);
PIN(offsetof(dmt_log_stream_t, flash_seg_count) == 12);
PIN(offsetof(dmt_log_stream_t, flash_rate_bytes_per_day) == 16);
PIN(offsetof(dmt_log_stream_t, flash_rate_burst_bytes) == 20);
PIN(offsetof(dmt_log_stream_t, ctl_key) == 24);
PIN(offsetof(dmt_log_stream_t, drop_counter_key) == 28);
PIN(offsetof(dmt_log_stream_t, principal) == 32); PIN(offsetof(dmt_log_stream_t, flush) == 34);
PIN(offsetof(dmt_log_stream_t, introduced_in) == 36);
PIN(offsetof(dmt_log_stream_t, backing) == 38);
PIN(offsetof(dmt_log_stream_t, allowed_backings) == 39);
PIN(offsetof(dmt_log_stream_t, level_ram) == 40); PIN(offsetof(dmt_log_stream_t, level_file) == 41);
PIN(offsetof(dmt_log_stream_t, reserved0) == 42);

/**
 * @brief   Check DMT_RIGHTS against the bit layout: key i in byte i/4, read bit 2*(i%4).
 * @version 1
 * @return  0 when every lookup is right, 1 otherwise.
 */
int main(void)
{
    /* key 0: read; key 1: write; key 2: none; key 3: read+write; key 4: write */
    const uint8_t row[2] = { 0x01u | (0x02u << 2) | (0x03u << 6), 0x02u };
    const uint8_t want[5] = { DMT_RIGHT_READ, DMT_RIGHT_WRITE, 0u,
                              DMT_RIGHT_READ | DMT_RIGHT_WRITE, DMT_RIGHT_WRITE };
    unsigned i;
    for (i = 0u; i < 5u; i++) {
        if (DMT_RIGHTS(row, i) != want[i]) {
            return 1;
        }
    }
    return 0;
}
