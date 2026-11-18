//! Contract-mode refusals: one model edit per structural, rights and bounds refusal, one per
//! previous-contract refusal, and the release review of new principals.

mod common;

use common::*;
use std::fs;
use std::path::Path;

/// One refusal case: model version, file, text to replace, replacement, expected message.
type Case = (
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

/// The pub telemetry topic's template and direction, the text the topic-var cases replace.
const TELEMETRY: &str = "template = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"";

/// The telemetry topic's header, template and direction, the text the placeholder cases replace
/// (a placeholder declared before it, the topic after).
const TELEMETRY_TOPIC: &str =
    "[[topics]]\nid = \"telemetry\"\ntemplate = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"";

/// The telemetry topic with nine vars, one over the bound.
const NINE_VARS: &str = "template = \"{a}{b}{c}{d}{e}{f}{g}{h}{i}\"\ndirection = \"pub\"\nvars = [{ name = \"a\", max_len = 1 }, { name = \"b\", max_len = 1 }, { name = \"c\", max_len = 1 }, { name = \"d\", max_len = 1 }, { name = \"e\", max_len = 1 }, { name = \"f\", max_len = 1 }, { name = \"g\", max_len = 1 }, { name = \"h\", max_len = 1 }, { name = \"i\", max_len = 1 }]";

/// Nine placeholders, one over the bound, all used by the telemetry topic.
const NINE_PLACEHOLDERS: &str = "[[placeholders]]\nname = \"a\"\nmax_len = 1\n\n[[placeholders]]\nname = \"b\"\nmax_len = 1\n\n[[placeholders]]\nname = \"c\"\nmax_len = 1\n\n[[placeholders]]\nname = \"d\"\nmax_len = 1\n\n[[placeholders]]\nname = \"e\"\nmax_len = 1\n\n[[placeholders]]\nname = \"f\"\nmax_len = 1\n\n[[placeholders]]\nname = \"g\"\nmax_len = 1\n\n[[placeholders]]\nname = \"h\"\nmax_len = 1\n\n[[placeholders]]\nname = \"i\"\nmax_len = 1\n\n[[topics]]\nid = \"telemetry\"\ntemplate = \"{a}{b}{c}{d}{e}{f}{g}{h}{i}\"\ndirection = \"pub\"";

/// Edits of the generic model that each trigger one refusal.
const CASES: &[Case] = &[
    ("v1_0", "ns/core.toml", "key_index = 0\ntype = \"bool\"", "type = \"bool\"", "missing field `key_index`"),
    ("v1_0", "ns/drive.toml", "unit = \"mm/s\"", "units = \"mm/s\"", "unknown field"),
    ("v1_0", "ns/net.toml", "namespace_id = 4", "namespace_id = 3", "share namespace_id"),
    ("v1_0", "ns/drive.toml", "key_index = 4", "key_index = 2", "encode to the same ID"),
    ("v1_0", "ns/core.toml", "namespace_id = 1", "namespace_id = 0", "encodes to ID 0, which is reserved forever"),
    ("v1_0", "ns/drive.toml", "id = \"enabled\"", "id = \"enabled\"\nread_only = true", "read_only and thread_safe"),
    ("v1_0", "ns/drive.toml", "key_index = 4", "key_index = 9", "is reserved"),
    ("v1_0", "ns/drive.toml", "write_rate = \"1/min\"\ndefault = 100", "default = 100", "needs a write_rate"),
    ("v1_0", "ns/drive.toml", "deferred_ms = 5000", "deferred_ms = 0", "deferred_ms > 0"),
    ("v1_0", "ns/drive.toml", "max_size = 65536\n", "", "needs max_size"),
    ("v1_0", "ns/drive.toml", "id = \"enabled\"", "id = \"enabled\"\nalias_of = \"drive.state.nope\"", "alias_of must name"),
    ("v1_0", "ns/drive.toml", "introduced_in = \"1.0\"\nwrite_rate = \"10/s\"", "introduced_in = \"1.1\"\nwrite_rate = \"10/s\"", "later than the contract"),
    ("v1_0", "ns/drive.toml", "default = 100", "default = 70000", "out of range"),
    ("v1_0", "ns/core.toml", "wk = \"SECRETS_FALLBACK\"", "", "SECRETS_FALLBACK is not mapped"),
    ("v1_0", "ns/core.toml", "type = \"uint32\"\nintroduced_in = \"1.0\"\npersistence = \"deferred\"", "type = \"uint16\"\nintroduced_in = \"1.0\"\npersistence = \"deferred\"", "core.diag.corruption: role DIAG_CORRUPTION needs type uint32 (has uint16)"),
    ("v1_0", "ns/mfg.toml", "mfg = true", "mfg = false", "MFG namespace"),
    ("v1_0", "ns/core.toml", "type = \"uint32\"\nintroduced_in = \"1.0\"\npersistence = \"deferred\"", "type = \"uint32\"\nintroduced_in = \"1.0\"\npersistence = \"durable\"", "core.diag.corruption: role DIAG_CORRUPTION needs persistence deferred (has durable)"),
    ("v1_0", "ns/core.toml", "survives_reset = true\n", "", "core.config.boot_epoch: role BOOT_EPOCH needs survives_reset = true (has false)"),
    ("v1_0", "ns/core.toml", "payload_max = 118", "payload_max = 0", "payload_max > 0"),
    ("v1_0", "ns/core.toml", "payload_max = 118", "payload_max = 117", "payload_max >= 118"),
    ("v1_0", "ns/core.toml", "max_size = 512", "max_size = 163", "DIAG_SUBSCRIBERS needs max_size >= 164"),
    ("v1_0", "ns/core.toml", "max_size = 32\nintroduced_in = \"1.0\"\nwk = \"DIAG_KEYSTORE_VERSION\"", "max_size = 29\nintroduced_in = \"1.0\"\nwk = \"DIAG_KEYSTORE_VERSION\"", "DIAG_KEYSTORE_VERSION needs max_size >= 30"),
    ("v1_0", "ns/core.toml", "max_size = 32\nintroduced_in = \"1.0\"\nwk = \"DIAG_CONTRACT_VERSION\"", "max_size = 16\nintroduced_in = \"1.0\"\nwk = \"DIAG_CONTRACT_VERSION\"", "DIAG_CONTRACT_VERSION needs max_size >= 17"),
    ("v1_0", "contract.toml", "uid = 2003\nkind", "uid = 2001\nkind", "(kind, uid, token)"),
    ("v1_0", "contract.toml", "heartbeat_window_ms = 2000", "heartbeat_window_ms = 0", "watchdog_required needs"),
    ("v1_0", "contract.toml", "staging_total_bytes = 262144", "staging_total_bytes = 1000", "staging_total_bytes"),
    ("v1_0", "contract.toml", "inproc_snapshot_bytes = 4096", "inproc_snapshot_bytes = 67", "inproc_snapshot_bytes 67 is below its lower bound 68"),
    ("v1_0", "contract.toml", "uid = 2003\ninflight_fd", "uid = 2999\ninflight_fd", "uid_caps must list"),
    ("v1_0", "contract.toml", "{ key = \"drive.state.speed\", access = \"rw\" },", "{ namespace = \"net\", class = \"credentials\", access = \"w\" },", "may not have rights on a secret"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { key = \"net.credentials.wifi_psk\", access = \"r\" },", "not on the secret's reader list"),
    ("v1_0", "contract.toml", "# and submits a command\n]", "# and submits a command\n  { namespace = \"drive\", class = \"map\", access = \"r\" },\n]", "may not have rights on a struct"),
    ("v1_1", "contract.toml", "  { key = \"drive.state.velocity\", access = \"rw\" },\n", "", "different rights on the alias"),
    ("v1_0", "contract.toml", "{ key = \"drive.state.speed\", access = \"rw\" },", "{ key = \"drive.state.speed\", access = \"rw\" },\n  { namespace = \"mfg\", access = \"w\" },", "mfg.identity.serial: UART principal mcu may not write"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { key = \"net.health.integrator_hb\", access = \"w\" },", "heartbeat of integrator"),
    ("v1_0", "contract.toml", "{ namespace = \"core\", class = \"diag\", access = \"r\" },\n]", "{ namespace = \"core\", class = \"diag\", access = \"rw\" },\n]", "written only by the core"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { key = \"core.command.trace_enable\", access = \"w\" },", "exactly one writer"),
    ("v1_0", "contract.toml", "{ key = \"core.command.pre_reset_result\", access = \"rw\" },", "{ key = \"core.command.pre_reset_result\", access = \"r\" },", "owner integrator needs"),
    ("v1_0", "ns/core.toml", "owner = \"core\"\ndedupe_ring = 4\nwrite_rate = \"1/day\"\nwk = \"RESET_REQUEST\"", "owner = \"integrator\"\ndedupe_ring = 4\nwrite_rate = \"1/day\"\nwk = \"RESET_REQUEST\"", "must be answered by the core"),
    ("v1_0", "ns/core.toml", "owner = \"integrator\"", "owner = \"core\"", "is only for RESET_REQUEST and TRACE_ENABLE"),
    ("v1_0", "contract.toml", "{ key = \"drive.command.map_edit_result\", access = \"rw\" },", "{ key = \"drive.command.map_edit_result\", access = \"rw\" },\n  { key = \"drive.command.map_edit_dedupe\", access = \"w\" },", "no principal may write a dedupe key"),
    ("v1_0", "contract.toml", "{ key = \"core.command.pre_reset_result\", access = \"rw\" },", "{ key = \"core.command.pre_reset_result\", access = \"rw\" },\n  { key = \"core.command.factory_reset_result\", access = \"w\" },", "answered by the core"),
    ("v1_0", "contract.toml", "{ key = \"drive.command.map_edit\", access = \"rw\" },", "{ key = \"drive.command.map_edit\", access = \"rw\" },\n  { key = \"drive.command.map_edit_result\", access = \"rw\" },", "drive.command.map_edit: only its owner app.map may write the result drive.command.map_edit_result; principal app.cloud has write on it"),
    ("v1_0", "contract.toml", "{ namespace = \"core\", class = \"diag\", access = \"r\" },\n]\n\n# -- a peer MCU", "{ namespace = \"core\", class = \"diag\", access = \"r\" },\n  { key = \"drive.command.drive_result\", access = \"w\" },\n]\n\n# -- a peer MCU", "drive.command.drive: only its owner integrator may write the result drive.command.drive_result; principal tool has write on it"),
    ("v1_0", "contract.toml", "name = \"tool\"", "name = \"core\"", "is reserved"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { key = \"drive.state.nope\", access = \"r\" },", "grant on unknown key"),
    ("v1_0", "ns/core.toml", "wk = \"SECRETS_FALLBACK\"", "wk = \"NOPE\"", "unknown well-known role"),
    ("v1_0", "ns/net.toml", "readers = [\"app.net\"]", "readers = [\"nobody\"]", "a reader is not a principal"),
    ("v1_0", "ns/drive.toml", "enum = \"mode\"", "enum = \"nope\"", "undefined enum"),
    ("v1_0", "ns/drive.toml", "id = \"enabled\"\nkey_index = 2\ntype = \"bool\"\nintroduced_in = \"1.0\"", "id = \"enabled\"\nkey_index = 2\ntype = \"bool\"\nintroduced_in = \"1\"", "drive.state: introduced_in '1' is not a"),
    ("v1_0", "ns/drive.toml", "write_rate = \"10/s\"", "write_rate = \"10/week\"", "write rate"),
    ("v1_0", "ns/drive.toml", "unit = \"mm/s\"", "unit = \"mm/s\"\nmax_size = 4", "max_size on a scalar"),
    ("v1_0", "ns/drive.toml", "key_index = 4", "key_index = 1024", "key_index 1024 exceeds 1023"),
    ("v1_0", "ns/core.toml", "cmd_index = 6", "cmd_index = 1024", "core.command.trace_enable: key_index 1024 exceeds 1023"),
    ("v1_0", "contract.toml", "{ key = \"drive.state.speed\", access = \"rw\" },", "{ key = \"drive.state.speed\", access = \"rw\" },\n  { key = \"core.command.pre_reset\", access = \"w\" },", "core.command.pre_reset: UART principal mcu may not write"),
    ("v1_0", "contract.toml", "{ key = \"drive.state.speed\", access = \"rw\" },", "{ key = \"drive.state.speed\", access = \"rw\" },\n  { key = \"core.command.trace_enable\", access = \"w\" },", "core.command.trace_enable: UART principal mcu may not write"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"r\" },", "net.health.app_hb: heartbeat of app must be written"),
    ("v1_0", "ns/drive.toml", "class_index = 2", "class_index = 1", "out of range or reused"),
    ("v1_0", "ns/drive.toml", "id = \"enabled\"", "id = \"enabled\"\nthread_safe = true", "read_only and thread_safe"),
    ("v1_0", "ns/drive.toml", "max_size = 32\nintroduced_in = \"1.0\"\ndefault = \"idle\"", "max_size = 0\nintroduced_in = \"1.0\"\ndefault = \"idle\"", "needs max_size > 0"),
    ("v1_0", "ns/core.toml", "namespace_id = 1", "namespace_id = 1024", "namespace_id 1024 exceeds 1023"),
    ("v1_0", "ns/drive.toml", "class_index = 2", "class_index = 32", "out of range or reused"),
    ("v1_0", "ns/drive.toml", "id = \"enabled\"", "id = \"enabled\"\nreaders = [\"app\"]", "only for secret keys"),
    ("v1_1", "ns/drive.toml", "key_index = 5\ntype = \"int16\"\nintroduced_in = \"1.1\"", "key_index = 5\ntype = \"int16\"\nintroduced_in = \"1.1\"\nalias_of = \"drive.state.speed\"", "alias_of must name"),
    ("v1_0", "ns/drive.toml", "id = \"enabled\"", "id = \"enabled\"\nalias_of = \"drive.state.speed\"", "alias_of must name"),
    ("v1_0", "ns/core.toml", "wk = \"DIAG_FSYNC_COUNT\"", "wk = \"DIAG_CORRUPTION\"", "mapped more than once"),
    ("v1_0", "ns/core.toml", "persistence = \"none\"\nowner = \"core\"\npayload_max = 118", "persistence = \"durable\"\nowner = \"core\"\npayload_max = 118", "non-persistent binary command"),
    ("v1_0", "ns/core.toml", "payload_max = 118", "payload = \"struct\"\npayload_max = 118", "non-persistent binary command"),
    ("v1_0", "contract.toml", "name = \"tool\"", "name = \"app\"", "is used twice or is reserved"),
    ("v1_0", "contract.toml", "staging_bound_bytes = 16384\nintroduced_in = \"1.0\"\ngrants = [\n  { namespace = \"core\", class = \"diag\", access = \"r\" },\n]", "staging_bound_bytes = 16384\nintroduced_in = \"1.1\"\ngrants = [\n  { namespace = \"core\", class = \"diag\", access = \"r\" },\n]", "principal tool: introduced_in is later"),
    ("v1_0", "contract.toml", "heartbeat = \"net.health.integrator_hb\"", "heartbeat = \"net.health.nope\"", "heartbeat is not a key"),
    ("v1_0", "contract.toml", "heartbeat = \"net.health.integrator_hb\"\n", "", "watchdog_required needs"),
    ("v1_0", "contract.toml", "[[uid_caps]]\nuid = 2003", "[[uid_caps]]\nuid = 2002\ninflight_fd = 1\ninflight_bytes = 1\nopen_fds = 1\n\n[[uid_caps]]\nuid = 2003", "uid_caps must list"),
    ("v1_0", "contract.toml", "{ key = \"drive.state.speed\", access = \"rw\" },", "{ key = \"drive.state.speed\", access = \"rw\" },\n  { namespace = \"drive\", class = \"map\", access = \"r\" },", "may not have rights on a struct"),
    ("v1_0", "contract.toml", "{ key = \"drive.state.speed\", access = \"rw\" },", "{ key = \"drive.state.speed\", access = \"rw\" },\n  { key = \"core.command.factory_reset\", access = \"w\" },", "core.command.factory_reset: UART principal mcu may not write"),
    ("v1_0", "contract.toml", "flags = [\"trace_reader\"]", "flags = [\"trace_reader\"]\nheartbeat = \"net.health.app_hb\"\nheartbeat_window_ms = 5000", "heartbeat of"),
    ("v1_0", "contract.toml", "  { key = \"core.command.trace_enable\", access = \"rw\" },\n", "", "exactly one writer"),
    ("v1_0", "ns/core.toml", "owner = \"integrator\"", "owner = \"nobody\"", "owner nobody is not a principal"),
    ("v1_0", "contract.toml", "  { key = \"core.command.pre_reset\", access = \"r\" },\n", "", "owner integrator needs"),
    ("v1_0", "contract.toml", "inproc_queue_bytes = 8192", "inproc_queue_bytes = 511", "inproc_queue_bytes 511 is below its lower bound 512"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { namespace = \"drive\", key = \"drive.state.speed\", access = \"r\" },", "names either a key or a namespace"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { class = \"state\", access = \"r\" },", "names either a key or a namespace"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { namespace = \"drvie\", access = \"r\" },", "grant on drvie covers no key"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { namespace = \"drive\", class = \"stat\", access = \"r\" },", "grant on drive.stat covers no key"),
    ("v1_0", "ns/net.toml", "id = \"net\"", "id = \"drive\"", "declared by two files"),
    ("v1_0", "ns/drive.toml", "error = { group = \"product\", bit = 1 }", "error = { group = \"prodcut\", bit = 1 }", "drive.state.error_overcurrent: error group prodcut is not declared"),
    ("v1_0", "ns/drive.toml", "id = \"enabled\"", "id = \"enabled\"\nerror = { group = \"product\", bit = 1 }", "bit 1 of error group product is taken"),
    ("v1_0", "ns/drive.toml", "id = \"speed_limit\"", "id = \"speed_limit\"\nerror = { group = \"product\", bit = 2 }", "drive.settings.speed_limit: an error key is a bool STATE key"),
    ("v1_0", "ns/drive.toml", "error = { group = \"product\", bit = 1 }", "error = { group = \"product\", bit = 0 }", "drive.state.error_overcurrent: bit 0 is reserved or not below width 32 of group product"),
    ("v1_0", "ns/drive.toml", "error = { group = \"product\", bit = 1 }", "error = { group = \"product\", bit = 32 }", "bit 32 is reserved or not below width 32"),
    ("v1_0", "contract.toml", "{ key = \"net.health.app_hb\", access = \"rw\" },", "{ key = \"net.health.app_hb\", access = \"rw\" },\n  { key = \"drive.state.error_overcurrent\", access = \"w\" },", "drive.state.error_overcurrent: an error key is written by integrator alone"),
    ("v1_0", "contract.toml", "wire_base = 32\n", "wire_base = 16\n", "error groups product and dock have overlapping wire ids"),
    ("v1_0", "contract.toml", "id = \"dock\"\nwidth = 32", "id = \"dock\"\nwidth = 12", "error group dock: width 8, 16, 32 or 64"),
    ("v1_0", "contract.toml", "reserved_bits = [0]", "reserved_bits = [40]", "error group product: width 8, 16, 32 or 64, bits below it"),
    ("v1_0", "contract.toml", "id = \"dock\"\nwidth = 32", "id = \"dock\"\nwidth = 32\npublish_max_bit = 32", "error group dock: width"),
    ("v1_0", "ns/drive.toml", "variants = [\"sku_b\"]", "variants = [\"sku_c\"]", "drive.state.error_dock_jam: variant sku_c is not declared"),
    ("v1_0", "ns/drive.toml", "owner = \"app.map\"", "owner = \"app.map\"\nvariants = [\"sku_c\"]", "drive.command.map_edit: variant sku_c is not declared"),
    ("v1_0", "contract.toml", "id = \"dock\"\nwidth = 32\nwire_base = 32\nwire_bases = { sku_b = 50 }          # sku_b numbers its dock errors from 50\nwriter = \"integrator\"", "id = \"dock\"\nwidth = 32\nwire_base = 32\nwire_bases = { sku_b = 50 }          # sku_b numbers its dock errors from 50\nwriter = \"nobody\"", "error group dock: width 8, 16, 32 or 64, bits below it, a principal writer"),
    ("v1_0", "contract.toml", "id = \"dock\"\nwidth = 32", "id = \"product\"\nwidth = 32", "error group product: width 8, 16, 32 or 64, bits below it, a principal writer and a unique id"),
    ("v1_0", "ns/core.toml", "wk = \"SECRETS_FALLBACK\"", "wk = \"SECRETS_FALLBACK\"\nerror = { group = \"product\", bit = 5 }", "core.diag.secrets_fallback: an error key is a bool STATE key"),
    ("v1_0", "ns/drive.toml", "id = \"odometer\"", "id = \"odometer\"\nerror = { group = \"product\", bit = 5 }", "drive.state.odometer: an error key is a bool STATE key"),
    ("v1_0", "ns/drive.toml", "unit = \"mm/s\"\nmin", "unit = \"mph\"\nmin", "drive.settings.speed_limit: unit mph is not in the unit vocabulary"),
    ("v1_0", "ns/drive.toml", "max = 2000\n", "max = 2005\n", "range 0..2005 step 10 must lie in"),
    ("v1_0", "ns/drive.toml", "max = 2000\n", "max = 70000\n", "range 0..70000 step 10 must lie in 0..65535"),
    ("v1_0", "ns/drive.toml", "step = 10\n", "step = 0\n", "range 0..2000 step 0 must lie in"),
    ("v1_0", "ns/drive.toml", "step = 10\n", "step = -10\n", "range 0..2000 step -10 must lie in"),
    ("v1_0", "ns/drive.toml", "id = \"odometer\"", "id = \"odometer\"\nallowed = []", "drive.state.odometer: allowed must be a non-empty set"),
    ("v1_0", "ns/drive.toml", "min = 0\nmax = 2000\nstep = 10\n", "unset = 100\n", "drive.settings.speed_limit: unset 100 must equal the default and lie outside"),
    ("v1_0", "ns/drive.toml", "min = 0\nmax = 2000\nstep = 10\n", "min = 200\nmax = 2000\nstep = 10\nunset = 50\n", "drive.settings.speed_limit: unset 50 must equal the default and lie outside"),
    ("v1_0", "ns/drive.toml", "min = 0\nmax = 2000", "min = 2000\nmax = 0", "range 2000..0 step 10 must lie in"),
    ("v1_0", "ns/drive.toml", "max = 2000\n", "", "drive.settings.speed_limit: a range needs both min and max"),
    ("v1_0", "ns/drive.toml", "default = 100", "default = 105", "drive.settings.speed_limit: default 105 is outside the range or allowed set"),
    ("v1_0", "ns/drive.toml", "defaults = { sku_b = 80 }", "defaults = { sku_b = 85 }", "drive.settings.speed_limit: default 85 is outside the range or allowed set"),
    ("v1_0", "ns/drive.toml", "enum = \"mode\"\n", "enum = \"mode\"\nmin = 0\nmax = 2\n", "drive.settings.mode: range, allowed, unset and scale are for integer keys without an enum"),
    ("v1_0", "ns/drive.toml", "id = \"label\"", "id = \"label\"\nscale = 2.0", "drive.state.label: range, allowed, unset and scale are for integer keys"),
    ("v1_0", "ns/drive.toml", "id = \"odometer\"", "id = \"odometer\"\nscale = 2.0", "drive.state.odometer: a scaled key names its displayed unit"),
    ("v1_0", "ns/drive.toml", "step = 10\n", "step = 10\nscale = 0.0\n", "drive.settings.speed_limit: scale 0 must be a positive finite number"),
    ("v1_0", "ns/drive.toml", "step = 10\n", "step = 10\nallowed = [100]\n", "allowed must be a non-empty set of distinct values"),
    ("v1_0", "ns/drive.toml", "id = \"odometer\"", "id = \"odometer\"\nallowed = [1, 1]", "drive.state.odometer: allowed must be a non-empty set of distinct values"),
    ("v1_0", "ns/drive.toml", "id = \"odometer\"", "id = \"odometer\"\nallowed = [-1]", "drive.state.odometer: allowed must be a non-empty set of distinct values"),
    ("v1_0", "ns/drive.toml", "step = 10\n", "step = 10\nunset = 100\n", "drive.settings.speed_limit: unset 100 must equal the default and lie outside the range or allowed set"),
    ("v1_0", "ns/drive.toml", "id = \"odometer\"", "id = \"odometer\"\nunset = 0", "drive.state.odometer: unset 0 must equal the default and lie outside"),
    ("v1_0", "ns/drive.toml", "defaults = { sku_b = 80 }", "defaults = { sku_c = 80 }", "drive.settings.speed_limit: variant sku_c is not declared (declared: sku_a, sku_b)"),
    ("v1_0", "ns/drive.toml", "[enums.mode.variants.sku_b]  ", "[enums.mode.variants.sku_c]  ", "drive::mode: variant sku_c is not declared"),
    ("v1_0", "ns/drive.toml", "remove = [\"sport\"]", "remove = [\"sprot\"]", "drive::mode (sku_b): the variant removes sprot, which is not a base value"),
    ("v1_0", "ns/drive.toml", "saver = 1", "saver = 3", "drive::mode (sku_b): number 3 has two names"),
    ("v1_0", "ns/drive.toml", "wire = \"sportMode\"", "wire = \"eco\"", "drive::mode (sku_a): wire string eco is used twice"),
    // -- shadow wire mapping --------------------------------------
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"", "shadow = \"state..docked\"", "drive.state.docked: malformed shadow path 'state..docked'"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"", "shadow = \"state.tones\"", "drive.state.docked: shadow path state.tones is taken by drive.state.tones"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"", "shadow = \"state.tones.x\"", "drive.state.docked: shadow path state.tones.x is both a value and an object"),
    ("v1_0", "contract.toml", "path = \"sv\"", "path = \"state\"", "shadow_constant: shadow path state is both a value and an object"),
    ("v1_0", "contract.toml", "path = \"settings.planExists\"", "path = \"settings.fanSpeed\"", "shadow_echo: shadow path settings.fanSpeed is taken by drive.settings.fan"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"", "shadow = \"state.errors\"", "error groups: shadow path state.errors is taken by drive.state.docked"),
    ("v1_0", "ns/core.toml", "wk = \"SECRETS_FALLBACK\"", "wk = \"SECRETS_FALLBACK\"\nshadow = \"state.fallback\"", "core.diag.secrets_fallback: a shadow path is for SETTINGS, STATE and CONFIG keys"),
    ("v1_0", "ns/drive.toml", "id = \"map_data\"", "id = \"map_data\"\nshadow = \"state.map\"", "drive.map.map_data: a binary or struct key has no shadow value"),
    ("v1_0", "ns/drive.toml", "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]", "shadow = [\"state.fanSpeed\", \"state.fanSpeed\"]", "drive.state.fan_applied: a shadow path is listed twice"),
    ("v1_0", "ns/drive.toml", "inbound_unknown = \"quiet\"", "inbound_unknown = \"quiet\"\nforced = true", "drive.settings.fan: forced is for shadowed STATE keys"),
    ("v1_0", "ns/drive.toml", "default = \"idle\"", "default = \"idle\"\nforced = true", "drive.state.label: forced is for shadowed STATE keys"),
    ("v1_0", "ns/drive.toml", "max_items = 4", "max_items = 0", "drive.state.tones: render is for shadowed STATE string keys, with max_items > 0"),
    ("v1_0", "ns/drive.toml", "id = \"docked\"", "id = \"docked\"\nrender = { kind = \"csv_index_list\", enum = \"tone\", max_items = 4 }", "drive.state.docked: render is for shadowed STATE string keys"),
    ("v1_0", "ns/drive.toml", "default = \"idle\"", "default = \"idle\"\nrender = { kind = \"csv_index_list\", enum = \"tone\", max_items = 4 }", "drive.state.label: render is for shadowed STATE string keys"),
    ("v1_0", "ns/drive.toml", "enum = \"tone\"", "enum = \"tune\"", "drive.state.tones: undefined enum 'tune'"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.speed_limit\"", "applies = \"drive.settings.speed_limit\"\non_reject = \"echo\"", "drive.state.speed_limit_applied: on_reject is for shadowed SETTINGS keys"),
    ("v1_0", "ns/drive.toml", "enum = \"mode\"\n", "enum = \"mode\"\non_reject = \"echo\"\n", "drive.settings.mode: on_reject is for shadowed SETTINGS keys"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.speed_limit\"", "applies = \"drive.settings.mode\"", "drive.state.speed_limit_applied: applies must be on a STATE key and name a SETTINGS key of the same type and enum"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.fan\"", "applies = \"drive.settings.mode\"", "drive.state.fan_applied: applies must be on a STATE key"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.fan\"", "applies = \"drive.settings.nope\"", "drive.state.fan_applied: applies must be on a STATE key"),
    ("v1_0", "ns/drive.toml", "on_reject = \"clear_desired\"", "on_reject = \"clear_desired\"\napplies = \"drive.settings.fan\"", "drive.settings.speed_limit: applies must be on a STATE key"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.fan\"\n", "", "drive.settings.fan: a shadowed SETTINGS key needs one reported STATE twin that applies it (has 0)"),
    ("v1_0", "ns/drive.toml", "type = \"uint16\"\nintroduced_in = \"1.0\"\nshadow = \"settings.speedLimit\"\n", "type = \"uint16\"\nintroduced_in = \"1.0\"\n", "drive.settings.speed_limit: a shadowed SETTINGS key needs one reported STATE twin that applies it (has 0)"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.speed_limit\"", "applies = \"drive.settings.speed_limit\"\n\n[[classes.keys]]\nid = \"limit_twin\"\nkey_index = 12\ntype = \"uint16\"\nintroduced_in = \"1.0\"\nshadow = \"state.limit\"\napplies = \"drive.settings.speed_limit\"", "drive.settings.speed_limit: a shadowed SETTINGS key needs one reported STATE twin that applies it (has 2)"),
    ("v1_0", "ns/drive.toml", "telem = \"dock\"", "telem = \"tones\"", "drive.state.docked: telem name 'tones' is empty or used twice"),
    ("v1_0", "ns/drive.toml", "telem = \"dock\"", "telem = \"\"", "drive.state.docked: telem name '' is empty or used twice"),
    ("v1_0", "contract.toml", "type = \"bool\"\n\n[[shadow_echo]]", "type = \"string\"\n\n[[shadow_echo]]", "shadow_echo settings.planExists: a string echo needs max_size > 0, and only it has one"),
    ("v1_0", "contract.toml", "type = \"bool\"\n\n[[shadow_echo]]", "type = \"string\"\nmax_size = 0\n\n[[shadow_echo]]", "shadow_echo settings.planExists: a string echo needs max_size > 0"),
    ("v1_0", "contract.toml", "type = \"float\"", "type = \"float\"\nmax_size = 4", "shadow_echo settings.trim: a string echo needs max_size > 0, and only it has one"),
    ("v1_0", "contract.toml", "command_key = \"drive.settings.gain\"", "command_key = \"drive.state.speed\"", "shadow_echo settings.trim: command_key must name a SETTINGS key"),
    ("v1_0", "contract.toml", "value = \"1.0-gen\"", "value = \"1.0-gen\"\nimage = \"contract_version\"", "shadow_constant sv: declares a bool, number or string value, or an image"),
    ("v1_0", "contract.toml", "value = \"1.0-gen\"", "value = [1]", "shadow_constant sv: declares a bool, number or string value, or an image"),
    ("v1_0", "contract.toml", "value = \"1.0-gen\"\n", "", "shadow_constant sv: declares a bool, number or string value, or an image"),
    ("v1_0", "contract.toml", "id = \"motion\"\n", "id = \"motion\"\n\n[[apply_groups]]\nid = \"motion\"\n", "apply group motion is declared twice"),
    ("v1_0", "ns/drive.toml", "quiet = { value = 0, wire = \"quiet\" }", "quiet = 0", "drive.settings.fan: enum value quiet needs an explicit wire string on a shadowed key"),
    ("v1_0", "ns/drive.toml", "beep = { value = 0, wire = \"beep\" }", "beep = 0", "drive.state.tones: enum value beep needs an explicit wire string on a shadowed key"),
    ("v1_0", "ns/drive.toml", "inbound_unknown = \"quiet\"            # an unmatched string is quiet (code 0), as today\n", "", "drive.settings.fan: a shadowed enum key declares inbound_unknown"),
    ("v1_0", "ns/drive.toml", "inbound_unknown = \"quiet\"", "inbound_unknown = \"bogus\"", "drive.settings.fan: inbound_unknown bogus is neither reject, ignore nor an inbound string of the enum"),
    ("v1_0", "ns/drive.toml", "inbound_unknown = \"quiet\"", "inbound_unknown = \"index\"", "drive.settings.fan: inbound_unknown index (an unchecked read) must be resolved before generation"),
    ("v1_0", "ns/drive.toml", "outbound_unknown = \"normal\"          # a code past the enum reports \"normal\"\n", "", "drive.state.fan_applied: a shadowed enum key declares outbound_unknown"),
    ("v1_0", "ns/drive.toml", "outbound_unknown = \"normal\"", "outbound_unknown = \"index\"", "drive.state.fan_applied: outbound_unknown index (an unchecked read) must be resolved before generation"),
    ("v1_0", "ns/drive.toml", "outbound_unknown = \"normal\"", "outbound_unknown = \"max\"", "drive.state.fan_applied: outbound_unknown max is neither raw nor a wire string of the enum"),
    ("v1_0", "ns/drive.toml", "outbound_unknown = \"normal\"", "outbound_unknown = \"normal\"\ninbound_unknown = \"quiet\"", "drive.state.fan_applied: inbound_unknown is for shadowed SETTINGS enum keys"),
    ("v1_0", "ns/drive.toml", "inbound_unknown = \"quiet\"", "inbound_unknown = \"quiet\"\noutbound_unknown = \"normal\"", "drive.settings.fan: outbound_unknown is for shadowed STATE and CONFIG enum keys"),
    ("v1_0", "ns/drive.toml", "enum = \"mode\"\n", "enum = \"mode\"\ninbound_unknown = \"off\"\n", "drive.settings.mode: inbound_unknown is for shadowed SETTINGS enum keys"),
    ("v1_0", "ns/drive.toml", "on_reject = \"clear_desired\"", "on_reject = \"clear_desired\"\ninbound_unknown = \"x\"", "drive.settings.speed_limit: inbound_unknown is for shadowed SETTINGS enum keys"),
    ("v1_0", "ns/drive.toml", "shadow = \"settings.fanSpeed\"\napply_group", "shadow = { path = \"settings.fanSpeed\", as = \"integer\" }\napply_group", "drive.settings.fan: inbound_unknown is for shadowed SETTINGS enum keys with a wire-string path"),
    ("v1_0", "ns/drive.toml", "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]", "shadow = [{ path = \"state.fanSpeed\", as = \"integer\" }, { path = \"settings.fanSpeed\", as = \"integer\" }]", "drive.state.fan_applied: outbound_unknown is for shadowed STATE and CONFIG enum keys with a wire-string path"),
    ("v1_0", "ns/drive.toml", "shadow = \"settings.speedLimit\"\napplies", "shadow = { path = \"settings.speedLimit\", as = \"integer\" }\napplies", "drive.state.speed_limit_applied: as is for enum and bool keys"),
    ("v1_0", "ns/drive.toml", "shadow = \"settings.speedLimit\"\napplies", "shadow = { path = \"settings.speedLimit\", as = \"enum\" }\napplies", "drive.state.speed_limit_applied: as is for enum and bool keys"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state.docked\", as = \"enum\" }\nforced", "drive.state.docked: as = \"enum\" is for enum keys"),
    ("v1_0", "ns/drive.toml", "[[classes]]\nid = \"state\"", "[[classes.keys]]\nid = \"flag\"\nkey_index = 9\ntype = \"bool\"\nintroduced_in = \"1.0\"\nshadow = { path = \"settings.flag\", as = \"integer\" }\n\n[[classes]]\nid = \"state\"", "drive.settings.flag: as = \"integer\" on a bool key is for reported paths"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = [{ path = \"state.docked\", scope = \"full\" }, { path = \"state.docked\", scope = \"both\" }]\nforced", "drive.state.docked: a shadow path is listed twice"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = [{ path = \"state.docked\", scope = \"delta\" }, { path = \"state.docked\", as = \"integer\", scope = \"delta\" }]\nforced", "drive.state.docked: a shadow path is listed twice"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = [\"state.docked\", { path = \"settings.planExists\", as = \"integer\", scope = \"delta\" }]\nforced", "shadow_echo: shadow path settings.planExists is taken by drive.state.docked"),
    ("v1_0", "contract.toml", "[[shadow_constant]]\npath = \"state.connected\"", "[shadow_rejected]\npath = \"state.docked\"\n\n[[shadow_constant]]\npath = \"state.connected\"", "shadow_rejected: shadow path state.docked is taken by drive.state.docked"),
    ("v1_0", "contract.toml", "[[shadow_constant]]\npath = \"state.connected\"", "[shadow_rejected]\npath = \"state..rejected\"\n\n[[shadow_constant]]\npath = \"state.connected\"", "shadow_rejected: malformed shadow path 'state..rejected'"),
    ("v1_0", "contract.toml", "[[shadow_constant]]\npath = \"state.connected\"", "[shadow_rejected]\npath = \"state.rejected\"\nvariants = [\"sku_c\"]\n\n[[shadow_constant]]\npath = \"state.connected\"", "shadow_rejected state.rejected: variant sku_c is not declared"),
    ("v2_0", "contract.toml", "[[uid_caps]]\nuid = 2001", "[shadow_rejected]\npath = \"state.rejected\"\n\n[[uid_caps]]\nuid = 2001", "shadow_rejected: the model has no desired path to reject"),
    ("v1_0", "ns/drive.toml", "shadow = \"settings.fanSpeed\"\napply_group", "shadow = { path = \"settings.fanSpeed\", scope = \"both\" }\napply_group", "drive.settings.fan: scope is for reported paths of STATE and CONFIG keys"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state.docked\", scope = \"full\" }\nforced", "drive.state.docked: forced needs a path in the delta"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state.fanSpeed\", scope = \"delta\" }\nforced", "drive.state.docked: shadow path state.fanSpeed is taken by drive.state.fan_applied"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = [{ path = \"state.fanSpeed\", scope = \"full\" }, \"state.docked\"]\nforced", "drive.state.docked: shadow path state.fanSpeed is taken by drive.state.fan_applied"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state..docked\", scope = \"delta\" }\nforced", "drive.state.docked: malformed shadow path 'state..docked'"),
    ("v1_0", "ns/drive.toml", "id = \"odometer\"\nkey_index = 1", "id = \"odometer\"\nkey_index = 1\nshadow = { path = \"settings.planExists\", scope = \"delta\" }", "shadow_echo: shadow path settings.planExists is taken by drive.state.odometer"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state.connected\", scope = \"delta\" }\nforced", "shadow_constant: shadow path state.connected is taken by drive.state.docked"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state.docked\", as = \"text\" }\nforced", "did not match any variant"),
    ("v1_0", "ns/drive.toml", "accept = [\"quiet\"] }", "accept = [\"quiet\", \"normal\"] }", "drive::fan (sku_b): inbound string normal maps to two codes"),
    ("v1_0", "ns/drive.toml", "accept = [\"quiet\"] }", "accept = [\"quiet\", \"max\"] }", "drive::fan (sku_b): inbound string max maps to two codes"),
    ("v1_0", "ns/drive.toml", "apply_group = \"motion\"\ninbound_unknown", "apply_group = \"motoin\"\ninbound_unknown", "drive.settings.fan: apply group motoin is not declared"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.speed_limit\"", "applies = \"drive.settings.speed_limit\"\napply_group = \"motion\"", "drive.state.speed_limit_applied: apply_group is for SETTINGS keys"),
    ("v1_0", "ns/drive.toml", "{ key = \"drive.settings.speed_limit\", value = 500 }", "{ key = \"drive.settings.mode\", value = 1 }", "drive.settings.fan: implies drive.settings.mode is outside the key's apply group"),
    ("v1_0", "ns/drive.toml", "{ key = \"drive.settings.speed_limit\", value = 500 }", "{ key = \"drive.settings.fan\", value = 1 }", "drive.settings.fan: implies drive.settings.fan is outside the key's apply group"),
    ("v1_0", "ns/drive.toml", "{ key = \"drive.settings.speed_limit\", value = 500 }", "{ key = \"drive.settings.nope\", value = 1 }", "drive.settings.fan: implies drive.settings.nope is outside the key's apply group"),
    ("v1_0", "ns/drive.toml", "{ key = \"drive.settings.speed_limit\", value = 500 }", "{ key = \"drive.settings.speed_limit\", value = 70000 }", "drive.settings.fan: implies drive.settings.speed_limit: default 70000 is out of range"),
    ("v1_0", "ns/drive.toml", "enum = \"mode\"\n", "enum = \"mode\"\nimplies = [ { key = \"drive.settings.gain\", value = 1.0 } ]\n", "drive.settings.mode: implies drive.settings.gain is outside the key's apply group"),
    ("v1_0", "ns/drive.toml", "max_size = 32\nintroduced_in = \"1.0\"\ndefault = \"idle\"", "max_size = 1400\nintroduced_in = \"1.0\"\ndefault = \"idle\"\nshadow = \"state.label\"", "over the 8192-byte AWS IoT shadow quota"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"stop\" }", "selector = { field = \"state\" }", "the selector has one of wire, element or object"),
    ("v1_0", "ns/drive.toml", "on_reject = \"clear_desired\"", "on_reject = \"clear_desired\"\napplies = \"drive.settings.speed_limit\"", "drive.settings.speed_limit: applies must be on a STATE key"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.speed_limit\"", "applies = \"drive.state.speed_limit_applied\"", "drive.state.speed_limit_applied: applies must be on a STATE key and name a SETTINGS key"),
    ("v1_0", "ns/drive.toml", "applies = \"drive.settings.speed_limit\"", "applies = \"drive.settings.gain\"", "drive.state.speed_limit_applied: applies must be on a STATE key and name a SETTINGS key of the same type"),
    // -- topics and envelopes -------------------------------------------
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/telemetry\"", "template = \"{stage}/{dui}/telemetry\"", "topic telemetry: template {stage}/{dui}/telemetry uses a placeholder outside {stage}, {duid}, {model}, {thing}, {shadow_name}"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/telemetry\"", "template = \"{stage}/duid}/telemetry\"", "topic telemetry: template {stage}/duid}/telemetry uses a placeholder outside"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/telemetry\"", "template = \"st}age/{duid}/telemetry\"", "topic telemetry: template st}age/{duid}/telemetry uses a placeholder outside"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry/{session}\"\ndirection = \"pub\"\nvars = [{ name = \"job\", max_len = 8 }]", "topic telemetry: template {stage}/{duid}/telemetry/{session} uses a placeholder outside {stage}, {duid}, {model}, {thing}, {shadow_name}, the declared placeholders and the topic's vars"),
    ("v1_0", "contract.toml", TELEMETRY_TOPIC, "[[placeholders]]\nname = \"sku\"\nmax_len = 32\n\n[[topics]]\nid = \"telemetry\"\ntemplate = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"", "placeholder sku is used by no topic"),
    ("v1_0", "contract.toml", TELEMETRY_TOPIC, "[[placeholders]]\nname = \"sku\"\nmax_len = 32\n\n[[placeholders]]\nname = \"sku\"\nmax_len = 8\n\n[[topics]]\nid = \"telemetry\"\ntemplate = \"{stage}/{sku}/telemetry\"\ndirection = \"pub\"", "placeholder sku is declared twice"),
    ("v1_0", "contract.toml", TELEMETRY_TOPIC, "[[placeholders]]\nname = \"stage\"\nmax_len = 32\n\n[[topics]]\nid = \"telemetry\"\ntemplate = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"", "placeholder stage must be an identifier other than a built-in placeholder"),
    ("v1_0", "contract.toml", TELEMETRY_TOPIC, "[[placeholders]]\nname = \"sku\"\nmax_len = 0\n\n[[topics]]\nid = \"telemetry\"\ntemplate = \"{stage}/{sku}/telemetry\"\ndirection = \"pub\"", "placeholder sku needs max_len 1..=128"),
    ("v1_0", "contract.toml", TELEMETRY_TOPIC, "[[placeholders]]\nname = \"sku\"\nmax_len = 32\n\n[[topics]]\nid = \"telemetry\"\ntemplate = \"{stage}/{sku}/telemetry\"\ndirection = \"pub\"\nvars = [{ name = \"sku\", max_len = 8 }]", "topic telemetry: var sku is a declared placeholder"),
    ("v1_0", "contract.toml", TELEMETRY_TOPIC, NINE_PLACEHOLDERS, "more than 8 placeholders"),
    // -- enum constants (dmt_constants.h) -------------------------------------------------------
    ("v1_0", "ns/drive.toml", "eco = 1\n", "eco = 1\na_value_name_long_enough_to_push_the_constant_past_the_limit = 7\n", "drive.settings.mode: enum constant DMT_E_DRIVE_SETTINGS_MODE_A_VALUE_NAME_LONG_ENOUGH_TO_PUSH_THE_CONSTANT_PAST_THE_LIMIT is longer than 63 characters"),
    ("v1_0", "ns/drive.toml", "eco = 1\n", "eco = 1\n\"eco-x\" = 7\n", "drive.settings.mode: enum constant DMT_E_DRIVE_SETTINGS_MODE_ECO-X is not a C identifier"),
    // -- command outcomes (dmt_constants.h) ----------------------------------------------------
    ("v1_0", "ns/drive.toml", "outcome = \"drive_outcome\"", "outcome = \"nope\"", "drive.command.drive: undefined enum 'nope'"),
    ("v1_0", "ns/drive.toml", "outcome = \"drive_outcome\"", "outcome = \"core::drive_outcome\"", "drive.command.drive: undefined enum 'core::drive_outcome'"),
    ("v1_0", "ns/drive.toml", "busy = 2\n", "busy = 2\na_value_name_long_enough_to_pass_the_limit = 9\n", "drive.command.drive: enum constant DMT_E_DRIVE_COMMAND_DRIVE_A_VALUE_NAME_LONG_ENOUGH_TO_PASS_THE_LIMIT is longer than 63 characters"),
    ("v1_0", "ns/drive.toml", "stale_offer = 3\n", "\"stale-offer\" = 3\n", "drive.command.drive: enum constant DMT_E_DRIVE_COMMAND_DRIVE_STALE-OFFER is not a C identifier"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry/{duid}\"\ndirection = \"pub\"\nvars = [{ name = \"duid\", max_len = 8 }]", "topic telemetry: var duid must be an identifier other than a built-in placeholder"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry/{a-b}\"\ndirection = \"pub\"\nvars = [{ name = \"a-b\", max_len = 8 }]", "topic telemetry: var a-b must be an identifier other than a built-in placeholder"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry/{job}\"\ndirection = \"pub\"\nvars = [{ name = \"job\", max_len = 8 }, { name = \"job\", max_len = 4 }]", "topic telemetry: var job is declared twice"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry/{job}\"\ndirection = \"pub\"\nvars = [{ name = \"job\", max_len = 0 }]", "topic telemetry: var job needs max_len 1..=128"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry/{job}\"\ndirection = \"pub\"\nvars = [{ name = \"job\", max_len = 129 }]", "topic telemetry: var job needs max_len 1..=128"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"\nvars = [{ name = \"job\", max_len = 8 }]", "topic telemetry: var job is not used by its template"),
    ("v1_0", "contract.toml", TELEMETRY, NINE_VARS, "topic telemetry: more than 8 vars"),
    ("v1_0", "contract.toml", TELEMETRY, "template = \"{stage}/{duid}/telemetry\"\ndirection = \"pub\"\nvariants = [\"sku_c\"]", "topic telemetry: variant sku_c is not declared"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/req\"\ndirection = \"sub\"", "template = \"{stage}/{duid}/cmd/req/j{job}\"\ndirection = \"sub\"\nvars = [{ name = \"job\", max_len = 8 }]", "topic command_topic: var job must be one whole topic level, used once, on a sub topic"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/req\"\ndirection = \"sub\"", "template = \"{stage}/{duid}/cmd/{job}/req/{job}\"\ndirection = \"sub\"\nvars = [{ name = \"job\", max_len = 8 }]", "topic command_topic: var job must be one whole topic level, used once, on a sub topic"),
    ("v1_0", "contract.toml", "id = \"telemetry\"", "id = \"command_resp\"", "topics: id command_resp is used twice"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/telemetry\"", "template = \"{stage}/{duid}/cmd/resp\"", "topics: template {stage}/{duid}/cmd/resp is used twice"),
    ("v1_0", "contract.toml", "subscribe_order = 3", "subscribe_order = 2", "topics: subscribe_order 2 is used twice"),
    ("v1_0", "contract.toml", "local_wrapper = \"mapjob_topic\"", "local_wrapper = \"command_topic\"", "topics: local_wrapper command_topic is used twice"),
    ("v1_0", "contract.toml", "connection = \"telemetry\"", "connection = \"telemetry\"\nsubscribe_order = 9", "topic telemetry: subscribe_order, local_wrapper, ack, envelope and settings_in_every_body are for sub topics"),
    ("v1_0", "contract.toml", "connection = \"telemetry\"", "connection = \"telemetry\"\nlocal_wrapper = \"telemetry\"", "topic telemetry: subscribe_order, local_wrapper, ack, envelope and settings_in_every_body are for sub topics"),
    ("v1_0", "contract.toml", "connection = \"telemetry\"", "connection = \"telemetry\"\nack = \"command_resp\"", "topic telemetry: subscribe_order, local_wrapper, ack, envelope and settings_in_every_body are for sub topics"),
    ("v1_0", "contract.toml", "connection = \"telemetry\"", "connection = \"telemetry\"\nenvelope = { }", "topic telemetry: subscribe_order, local_wrapper, ack, envelope and settings_in_every_body are for sub topics"),
    ("v1_0", "contract.toml", "connection = \"telemetry\"", "connection = \"telemetry\"\nsettings_in_every_body = true", "topic telemetry: subscribe_order, local_wrapper, ack, envelope and settings_in_every_body are for sub topics"),
    ("v1_0", "contract.toml", "ack = \"command_resp\"", "ack = \"mapjob_topic\"", "topic command_topic: ack mapjob_topic is not a pub topic"),
    ("v1_0", "contract.toml", "ack = \"command_resp\"", "ack = \"nope\"", "topic command_topic: ack nope is not a pub topic"),
    ("v1_0", "contract.toml", "id = { field = \"id\", required = true }, source = { field = \"source\"", "id = { field = \"id\" }, source = { field = \"source\"", "topic command_topic: the envelope id is required, or optional with an absent value"),
    ("v1_0", "contract.toml", "id = { field = \"id\", required = true }, source = { field = \"source\"", "id = { field = \"id\", required = true, absent = 0 }, source = { field = \"source\"", "topic command_topic: the envelope id is required, or optional with an absent value"),
    ("v1_0", "contract.toml", "enum = \"core::origin\", absent = \"unspecified\" } }\n\n[[topics]]\nid = \"command_resp\"", "enum = \"core::origin\", absent = \"nobody\" } }\n\n[[topics]]\nid = \"command_resp\"", "topic command_topic: the envelope source absent value nobody is not a member of enum core::origin"),
    ("v1_0", "contract.toml", "source = { field = \"source\", enum = \"core::origin\"", "source = { field = \"source\", enum = \"core::orign\"", "topic command_topic: the envelope source absent value unspecified is not a member of enum core::orign"),
    ("v1_0", "contract.toml", "template = \"$aws/things/{duid}/shadow/name/{shadow_name}/update/delta\"", "template = \"$aws/things/{duid}/shadow/name/device/update/delta\"", "topic shadow_delta: shadow name device is a literal while another topic uses {shadow_name}"),
    ("v1_0", "contract.toml", "local_wrapper = \"shadow_delta\"", "local_wrapper = \"shadow_delta\"\nliteral_shadow_name = true", "topic shadow_delta: literal_shadow_name is for a literal shadow name beside {shadow_name}"),
    // -- command rows -------------------------------------------------
    ("v1_0", "ns/core.toml", "topic = \"command_topic\"\nselector = { field = \"net.factoryReset\"", "topic = \"nope\"\nselector = { field = \"net.factoryReset\"", "shadow_commands nope net.factoryReset=true: topic nope is not a sub topic"),
    ("v1_0", "ns/core.toml", "topic = \"command_topic\"\nselector = { field = \"net.factoryReset\"", "topic = \"command_resp\"\nselector = { field = \"net.factoryReset\"", "shadow_commands command_resp net.factoryReset=true: topic command_resp is not a sub topic"),
    ("v1_0", "ns/drive.toml", "grounded_in = \"the sku_b library build\"\n", "", "shadow_commands command_topic state=approve: a per-variant row names the build it is grounded in, and only it"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }", "grounded_in = \"x\"\nselector = { field = \"state\", wire = \"go\" }", "shadow_commands command_topic state=go: a per-variant row names the build it is grounded in, and only it"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"stop\" }", "selector = { field = \"state\", wire = \"stop\", object = true }", "shadow_commands command_topic state=stop: the selector has one of wire, element or object"),
    ("v1_0", "ns/drive.toml", "protocol = \"command_array\"\nselector = { field = \"cmd\", element = \"newMap\" }", "selector = { field = \"cmd\", element = \"newMap\" }", "shadow_commands mapjob_topic cmd[newMap]: an element selector is the command_array protocol, and only it"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }", "protocol = \"command_array\"\nselector = { field = \"state\", wire = \"go\" }", "shadow_commands command_topic state=go: an element selector is the command_array protocol, and only it"),
    ("v1_0", "contract.toml", "envelope = { id = { field = \"id\", required = true }, source = { enum = \"core::origin\"", "envelope = { source = { enum = \"core::origin\"", "shadow_commands mapjob_topic cmd[newMap]: a command_array topic needs an envelope id"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"cmd\", wire = \"saveMap\" } }", "selector = { field = \"cmd\", element = \"saveMap\" } }", "shadow_commands command_topic state=mapcmd map.cmd[saveMap]: an object body selects its verb by wire"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"cmd\", wire = \"saveMap\" } }", "selector = { field = \"cmd\", wire = \"saveMap\", object = true } }", "shadow_commands command_topic state=mapcmd map.cmd=saveMap: an object body selects its verb by wire"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"stop\" }", "selector = { field = \"state\", wire = \"stop\" }\nbody = \"binary\"", "shadow_commands command_topic state=stop: no library build accepts a binary body; send base64 text in a field"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"settings\" }\nbody = \"settings\"", "selector = { field = \"state\", wire = \"settings\" }\nbody = \"settings\"\ncommand = \"drive.command.drive\"", "shadow_commands command_topic state=settings: a settings body has no command and no args"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"settings\" }\nbody = \"settings\"", "selector = { field = \"state\", wire = \"settings\" }\nbody = \"settings\"\nargs = [ { field = \"x\", key = \"drive.command.map_id\", absent = 0 } ]", "shadow_commands command_topic state=settings: a settings body has no command and no args"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }", "shadow_commands command_topic state=go: command (none) is not a command of this variant"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.nope\"", "shadow_commands command_topic state=go: command drive.command.nope is not a command of this variant"),
    ("v1_0", "ns/drive.toml", "owner = \"app.map\"", "owner = \"app.map\"\nvariants = [\"sku_b\"]", "shadow_commands mapjob_topic cmd[newMap]: command drive.command.map_edit is not a command of this variant"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.plan_epoch\"", "key = \"drive.command.drive\"", "shadow_commands command_topic state=approve: arg offerEpoch: drive.command.drive is neither a plain key of the command's class nor a STATE key"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.plan_epoch\"", "key = \"drive.command.nope\"", "arg offerEpoch: drive.command.nope is neither a plain key of the command's class nor a STATE key"),
    ("v1_0", "ns/drive.toml", "required = true, ec = 5 }", "required = \"per_handler\" }", "shadow_commands mapjob_topic cmd[splitArea]: arg areaid: required = per_handler must be resolved from the handler"),
    ("v1_0", "ns/drive.toml", "required = true, ec = 5 }", "required = true }", "arg areaid: a required argument has its ec and no absent rule"),
    ("v1_0", "ns/drive.toml", "required = true, ec = 5 }", "required = true, ec = 5, absent = 0 }", "arg areaid: a required argument has its ec and no absent rule"),
    ("v1_0", "ns/drive.toml", "required = true, ec = 5 }", "required = true, ec = 5, omit_when_absent = true }", "arg areaid: a required argument has its ec and no absent rule"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.plan_epoch\", absent = 0 }", "key = \"drive.command.plan_epoch\", absent = 0, ec = 4 }", "arg offerEpoch: an optional argument has an absent value or omit_when_absent, and no ec"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.plan_epoch\", absent = 0 }", "key = \"drive.command.plan_epoch\" }", "arg offerEpoch: an optional argument has an absent value or omit_when_absent, and no ec"),
    ("v1_0", "ns/drive.toml", "omit_when_absent = true, encoding = \"base64\" }", "omit_when_absent = true, absent = \"\", encoding = \"base64\" }", "arg data: an optional argument has an absent value or omit_when_absent, and no ec"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.plan_epoch\", absent = 0 }", "key = \"drive.command.plan_epoch\", absent = -1 }", "arg offerEpoch: default -1 is out of range"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.map_id\", absent = 0 } ]", "key = \"drive.command.map_id\", absent = 0, encoding = \"base64\" } ]", "shadow_commands command_topic state=mapcmd map.cmd=saveMap: arg mapid: base64 text goes into a string, binary or struct key"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"stop\" }", "selector = { field = \"state\", wire = \"go\" }", "shadow_commands command_topic state=go: the request is selected twice on one variant"),
    ("v1_0", "ns/drive.toml", "variants = [ \"sku_b\" ]", "variants = [ \"sku_c\" ]", "shadow_commands command_topic state=approve: variant sku_c is not declared"),
    ("v1_0", "ns/core.toml", "internal = \"sent by the core before a factory reset; never requested on the wire\"\n", "", "core.command.pre_reset: no shadow_commands row enumerates it (declare one, or internal = \"<reason>\")"),
    ("v1_0", "ns/core.toml", "wk = \"RESET_REQUEST\"", "wk = \"RESET_REQUEST\"\ninternal = \"x\"", "core.command.factory_reset: an internal command has a shadow_commands row"),
    ("v1_0", "ns/core.toml", "internal = \"sent by the core before a factory reset; never requested on the wire\"", "internal = \"  \"", "core.command.pre_reset: internal needs a reason"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.plan_epoch\"", "key = \"drive.settings.mode\"", "arg offerEpoch: drive.settings.mode is neither a plain key of the command's class nor a STATE key"),
    ("v1_0", "ns/drive.toml", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }\nboosted = { value = 3, wire = \"turbo\", report_as = \"boost\" }", "drive::fan: value boosted declares a wire string or report_as, not both and not neither"),
    ("v1_0", "ns/drive.toml", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }\nboosted = { value = 3 }", "drive::fan: value boosted declares a wire string or report_as, not both and not neither"),
    ("v1_0", "ns/drive.toml", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }\nboosted = { value = 3, report_as = \"nope\" }", "drive::fan: value boosted reports as nope, which is not another member"),
    ("v1_0", "ns/drive.toml", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }\nboosted = { value = 3, report_as = \"boosted\" }", "drive::fan: value boosted reports as boosted, which is not another member"),
    ("v1_0", "ns/drive.toml", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }\nboosted = { value = 3, report_as = \"boost\" }\ntop = { value = 4, report_as = \"boosted\" }", "drive::fan: value top reports as boosted, which itself reports as another"),
    ("v1_0", "ns/drive.toml", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }\nboosted = { value = 3, report_as = \"boost\", accept = [\"high\"] }", "drive::fan: inbound string high maps to two codes"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state.docked\", variants = [\"sku_c\"] }\nforced", "drive.state.docked: variant sku_c is not declared"),
    ("v1_0", "ns/drive.toml", "shadow = \"state.docked\"\nforced", "shadow = { path = \"state.errors\", scope = \"delta\" }\nforced", "error groups: shadow path state.errors is taken by drive.state.docked"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.map_id\", absent = 0 } ]", "key = \"drive.command.map_id\", absent = 0, encoding = \"decimal_text\" } ]\n\n[[shadow_commands]]\ntopic = \"command_topic\"\nselector = { field = \"state\", wire = \"label\" }\nargs = [ { field = \"l\", key = \"drive.state.label\", omit_when_absent = true, encoding = \"decimal_text\" } ]", "arg l: decimal_text goes into an integer key"),
    ("v1_0", "ns/drive.toml", "key = \"drive.command.plan_epoch\", absent = 0 }", "key = \"drive.command.plan_epoch\", absent = 0, value = 1 }", "arg offerEpoch: an argument takes a field or a constant value, not both and not neither"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\" }", "arg drive.command.plan_epoch: an argument takes a field or a constant value, not both and not neither"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\", value = 1, absent = 0 }", "arg drive.command.plan_epoch: a constant has no required rule, ec, absent rule, encoding or element"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\", value = 1, element = 0 }", "arg drive.command.plan_epoch: a constant has no required rule, ec, absent rule, encoding or element"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\", value = -1 }", "arg drive.command.plan_epoch: default -1 is out of range"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.state.speed\", value = \"x\" }", "arg drive.state.speed: default \"x\" is not an integer"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ field = \"m\", key = \"drive.state.fan_applied\", absent = \"nope\" }", "arg m: nope is not a value name of the key's enum"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"stop\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"stop\" }\ncommand = \"drive.command.drive\"\nanswer = 3", "shadow_commands command_topic state=stop: a row with a command declares neither answer nor internal"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"stop\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"stop\" }\ncommand = \"drive.command.drive\"\ninternal = \"x\"", "shadow_commands command_topic state=stop: a row with a command declares neither answer nor internal"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\nanswer = 0\ninternal = \"x\"", "shadow_commands command_topic state=go: a row declares answer or internal, not both"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\nanswer = 0\nargs = [ { field = \"l\", key = \"drive.state.label\", omit_when_absent = true } ]", "shadow_commands command_topic state=go: an answer or internal row has no args"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\ninternal = \"x\"\nargs = [ { field = \"l\", key = \"drive.state.label\", omit_when_absent = true } ]", "shadow_commands command_topic state=go: an answer or internal row has no args"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\ninternal = \" \"", "shadow_commands command_topic state=go: internal needs a reason"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"\ninternal_id = 3", "shadow_commands command_topic state=go: internal_id is for internal rows"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\ninternal = \"x\"\ninternal_id = 0", "shadow_commands command_topic state=go: internal_id is 1..=65535"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\nargs = [ { field = \"m\", key = \"drive.command.map_id\", absent = 0 } ]", "shadow_commands command_topic state=go: arg m: drive.command.map_id is neither a plain key of the command's class nor a STATE key"),
    ("v1_0", "ns/drive.toml", "whisper = { value = 0, wire = \"whisper\", accept = [\"quiet\"] }", "whisper = { value = 0, accept = [\"quiet\"] }", "drive::fan (sku_b): value whisper declares a wire string or report_as, not both and not neither"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"settings\" }\nbody = \"settings\"", "selector = { field = \"state\", wire = \"settings\" }\nbody = \"settings\"\nanswer = 0", "shadow_commands command_topic state=settings: a settings body has no command and no args"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\", value = 1, required = true }", "arg drive.command.plan_epoch: a constant has no required rule, ec, absent rule, encoding or element"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\", value = 1, ec = 3 }", "arg drive.command.plan_epoch: a constant has no required rule, ec, absent rule, encoding or element"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\", value = 1, omit_when_absent = true }", "arg drive.command.plan_epoch: a constant has no required rule, ec, absent rule, encoding or element"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ key = \"drive.command.plan_epoch\", value = 1, encoding = \"decimal_text\" }", "arg drive.command.plan_epoch: a constant has no required rule, ec, absent rule, encoding or element"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"\nalso = [ { command = \"drive.command.nope\" } ]", "shadow_commands command_topic state=go: also: command drive.command.nope is not a command of this variant"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\nargs = [ { field = \"l\", key = \"drive.state.label\", omit_when_absent = true } ]\nalso = [ { command = \"drive.command.drive\" } ]", "shadow_commands command_topic state=go: also follows the row's own command"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"\nalso = [ { command = \"core.command.factory_reset\", args = [ { field = \"e\", key = \"drive.command.plan_epoch\", absent = 0 } ] } ]", "shadow_commands command_topic state=go: arg e: drive.command.plan_epoch is neither a plain key of the command's class nor a STATE key"),
    ("v1_0", "ns/drive.toml", "selector = { field = \"state\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "selector = { field = \"state.\", wire = \"go\" }\ncommand = \"drive.command.drive\"", "shadow_commands command_topic state.=go: field 'state.' is not dotted identifiers (each dot is one nested object)"),
    ("v1_0", "ns/drive.toml", "body = { object = \"map\", selector = { field = \"cmd\", wire = \"saveMap\" } }", "body = { object = \"map-x\", selector = { field = \"cmd\", wire = \"saveMap\" } }", "field 'map-x' is not dotted identifiers"),
    ("v1_0", "ns/drive.toml", "body = { object = \"map\", selector = { field = \"cmd\", wire = \"saveMap\" } }", "body = { object = \"map\", selector = { field = \"1cmd\", wire = \"saveMap\" } }", "field '1cmd' is not dotted identifiers"),
    ("v1_0", "ns/drive.toml", "{ field = \"offerEpoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "{ field = \"plan-epoch\", key = \"drive.command.plan_epoch\", absent = 0 }", "field 'plan-epoch' is not dotted identifiers"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_topic\"\npath = \"sv\"\nvalue = 1", "topic_payload command_topic sv: topic command_topic is not a pub topic"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nvalue = 1\nkey = \"drive.state.odometer\"", "topic_payload command_resp sv: a member has one of key, value or image"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nkey = \"drive.state.nope\"", "topic_payload command_resp sv: key drive.state.nope is not a key of this variant"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nkey = \"drive.command.drive_result\"", "topic_payload command_resp sv: a binary or struct key has no JSON value"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nkey = \"drive.state.odometer\"\nas = \"integer\"", "topic_payload command_resp sv: as is for enum keys"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nkey = \"drive.settings.fan\"", "topic_payload command_resp sv: an enum key's wire-string member declares outbound_unknown"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nkey = \"drive.state.odometer\"\noutbound_unknown = \"raw\"", "topic_payload command_resp sv: outbound_unknown is for an enum key's wire-string member"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nkey = \"drive.settings.fan\"\noutbound_unknown = \"bogus\"", "topic_payload command_resp sv: outbound_unknown bogus is neither raw nor a wire string of the enum"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nvalue = 1\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nvalue = 2", "topic_payload command_resp sv: shadow path sv is taken by topic_payload command_resp sv"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"a..b\"\nvalue = 1", "topic_payload command_resp a..b: malformed shadow path 'a..b'"),
    ("v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"", "template = \"{stage}/{duid}/cmd/resp\"\ndirection = \"pub\"\n\n[[topic_payload]]\ntopic = \"command_resp\"\npath = \"sv\"\nvalue = 1\nvariants = [\"sku_z\"]", "topic_payload command_resp sv: variant sku_z is not declared"),
];

#[test]
fn every_refusal_has_a_triggering_model() {
    let dir = unique_dir("refusals");
    for (i, (version, file, from, to, expected)) in CASES.iter().enumerate() {
        let case = dir.join(format!("case{i}"));
        copy_dir(&model(version), &case);
        edit(&case.join(file), from, to);
        let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
        assert!(!ok, "case {i} ({expected}) must be refused");
        assert!(
            stderr.contains(expected),
            "case {i}: expected '{expected}' in:\n{stderr}"
        );
        assert!(
            !case.join("out/dmt_tables.c").exists(),
            "case {i}: nothing is generated"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

/// Edits of 1.1 (checked against 1.0) or 2.0 (against 1.1) that the previous-contract checks refuse.
/// The map_edit command of the generic model, and the same three paths, IDs and types declared
/// as plain keys: only the command membership differs.
const MAP_EDIT_COMMAND: &str = "[[classes.commands]]\nid = \"map_edit\"\ncmd_index = 0\nresult_index = 1\ndedupe_index = 2\nintroduced_in = \"1.0\"\npersistence = \"durable\"\nowner = \"app.map\"\npayload = \"struct\"\npayload_max = 32768\nresult_max = 256\nmax_edits = 32\ndedupe_ring = 8\nwrite_rate = \"1/min\"\n";
const MAP_EDIT_PLAIN: &str = "persistence = \"durable\"\n\n[[classes.keys]]\nid = \"map_edit\"\nkey_index = 0\ntype = \"struct\"\nmax_size = 65536\nintroduced_in = \"1.0\"\nwrite_rate = \"1/min\"\n\n[[classes.keys]]\nid = \"map_edit_result\"\nkey_index = 1\ntype = \"binary\"\nmax_size = 65536\nintroduced_in = \"1.0\"\nwrite_rate = \"1/min\"\n\n[[classes.keys]]\nid = \"map_edit_dedupe\"\nkey_index = 2\ntype = \"binary\"\nmax_size = 65536\nintroduced_in = \"1.0\"\nwrite_rate = \"1/min\"\n";

const PREVIOUS_CASES: &[(&str, &str, &str, &str, &str, &str)] = &[
    (
        "v1_1",
        "v1_0",
        "ns/drive.toml",
        "id = \"speed_limit\"",
        "id = \"speed_limit\"\npersistence = \"none\"",
        "drive.settings.speed_limit: ID",
    ),
    (
        "v1_1",
        "v1_0",
        "contract.toml",
        "flags = [\"trace_reader\"]",
        "flags = []",
        "principal tool was removed or its uid",
    ),
    (
        "v1_1",
        "v1_0",
        "contract.toml",
        "  { namespace = \"drive\", class = \"state\", access = \"r\" },\n  { key = \"drive.settings.mode\"",
        "  { key = \"drive.settings.mode\"",
        "rights on existing keys changed",
    ),
    (
        "v1_1",
        "v1_0",
        "ns/drive.toml",
        "key_index = 5\ntype = \"int16\"\nintroduced_in = \"1.1\"",
        "key_index = 5\ntype = \"int16\"\nintroduced_in = \"1.0\"",
        "a new key must have introduced_in 1.1",
    ),
    (
        "v1_1",
        "v1_0",
        "ns/drive.toml",
        "eco = 1\n",
        "",
        "drive.settings.mode: ID",
    ),
    ("v1_1", "v1_0", "contract.toml", "[[reserved]]\nid = 0x00C20480   # drive.state index 9 (bool): retired before 1.0\n", "", "reserved ID 0x00C20480 is no longer reserved"),
    ("v2_0", "v1_1", "ns/drive.toml", "id = \"enabled\"", "id = \"active\"", "is reused by drive.state.active"),
    ("v1_1", "v1_0", "ns/drive.toml", "enum = \"mode\"\nintroduced_in = \"1.0\"", "enum = \"mode\"\nintroduced_in = \"1.1\"", "drive.settings.mode: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", "enum = \"mode\"\nintroduced_in = \"1.0\"", "introduced_in = \"1.0\"", "drive.settings.mode: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", "wire = \"sportMode\"", "wire = \"sport_mode\"", "drive.settings.mode: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", "max = 2000\n", "max = 1000\n", "drive.settings.speed_limit: unit, scale or unset changed, or its range or allowed set narrowed"),
    ("v1_1", "v1_0", "ns/drive.toml", "step = 10\n", "step = 20\n", "drive.settings.speed_limit: unit, scale or unset changed"),
    ("v1_1", "v1_0", "ns/drive.toml", "unit = \"mm/s\"\nmin", "unit = \"%\"\nmin", "drive.settings.speed_limit: unit, scale or unset changed"),
    ("v1_1", "v1_0", "units.toml", "meaning = \"percent\"", "meaning = \"per cent\"", "unit % was removed or changed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "error = { group = \"product\", bit = 1 }", "error = { group = \"product\", bit = 2 }", "drive.state.error_overcurrent: ID, type, introduced_in, persistence, secret, survives-reset, interval, alias, error bit"),
    ("v1_1", "v1_0", "contract.toml", "wire_base = 32\n", "wire_base = 40\n", "error group dock was removed or changed in a minor"),
    ("v1_1", "v1_0", "contract.toml", "reserved_bits = [0]", "reserved_bits = []", "error group product was removed or changed in a minor"),
    ("v1_1", "v1_0", "ns/net.toml", "secret = true\nreaders = [\"app.net\", \"integrator\", \"app.ota\"]\n", "", "net.credentials.cloud_token: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", "id = \"speed_limit\"", "id = \"speed_limit\"\nsurvives_reset = true", "drive.settings.speed_limit: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", "deferred_ms = 5000", "deferred_ms = 6000", "drive.state.odometer: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", "max_size = 32\nintroduced_in = \"1.0\"\ndefault = \"idle\"", "max_size = 16\nintroduced_in = \"1.0\"\ndefault = \"idle\"", "drive.state.label: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", "id = \"enabled\"\nkey_index = 2", "id = \"enabled\"\nkey_index = 6", "drive.state.enabled: ID"),
    ("v1_1", "v1_1", "ns/drive.toml", "alias_of = \"drive.state.velocity\"\n", "", "drive.state.speed: ID"),
    ("v1_1", "v1_0", "ns/drive.toml", MAP_EDIT_COMMAND, MAP_EDIT_PLAIN, "drive.command.map_edit: ID"),
    ("v1_0", "v1_0", "ns/drive.toml", "id = \"error_overcurrent\"", "id = \"spare\"\nkey_index = 6\ntype = \"bool\"\nintroduced_in = \"1.0\"\n\n[[classes.keys]]\nid = \"error_overcurrent\"", "keys were added without a minor bump"),
    ("v1_1", "v1_0", "contract.toml", "name = \"tool\"", "name = \"tools\"", "principal tool was removed"),
    ("v1_1", "v1_0", "contract.toml", "uid = 1\nkind = \"uart\"", "uid = 2\nkind = \"uart\"", "principal mcu was removed"),
    ("v1_1", "v1_0", "contract.toml", "token = 3", "token = 5", "principal app.map was removed"),
    ("v1_1", "v1_0", "contract.toml", "uid = 10\nkind = \"inproc\"", "uid = 10\nkind = \"uart\"", "principal inproc was removed"),
    ("v1_1", "v1_0", "ns/drive.toml", "inbound_unknown = \"quiet\"", "inbound_unknown = \"max\"", "drive.settings.fan: its shadow mapping changed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "shadow = [\"state.fanSpeed\", \"settings.fanSpeed\"]", "shadow = \"state.fanSpeed\"", "drive.state.fan_applied: its shadow mapping changed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "shadow = \"state.tones\"", "shadow = { path = \"state.tones\", scope = \"full\" }", "drive.state.tones: its shadow mapping changed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "boost = { value = 2, wire = \"high\", accept = [\"max\"] }", "boost = { value = 2, wire = \"high\" }", "drive.settings.fan: its shadow mapping changed in a minor"),
    ("v1_1", "v1_0", "contract.toml", "path = \"settings.trim\"\ntype = \"float\"", "path = \"settings.trim\"\ntype = \"int\"", "shadow_echo settings.trim changed or was removed in a minor"),
    ("v1_1", "v1_0", "contract.toml", "value = \"1.0-gen\"", "value = \"1.1-gen\"", "shadow_constant sv changed or was removed in a minor"),
    ("v1_1", "v1_0", "contract.toml", "template = \"{stage}/{duid}/cmd/resp\"", "template = \"{stage}/{duid}/cmd/ack\"", "topic command_resp changed or was removed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "args = [ { field = \"mapid\", key = \"drive.command.map_id\", absent = 0 } ]", "args = [ { field = \"mapid\", key = \"drive.command.map_id\", absent = 1 } ]", "shadow_commands command_topic state=mapcmd changed or was removed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "refused = 1\n", "refused = 4\n", "drive.command.drive: its outcomes were removed or changed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "busy = 2\n", "", "drive.command.drive: its outcomes were removed or changed in a minor"),
    ("v1_1", "v1_0", "ns/drive.toml", "outcome = \"drive_outcome\"\n", "", "drive.command.drive: its outcomes were removed or changed in a minor"),
    (
        "v2_0",
        "v1_1",
        "contract.toml",
        "[[reserved]]\nid = 0x00C201C0",
        "",
        "was removed but its ID",
    ),
];

#[test]
fn previous_contract_refusals() {
    let dir = unique_dir("previous");
    for (i, (version, prev, file, from, to, expected)) in PREVIOUS_CASES.iter().enumerate() {
        let (case, prev_out) = (dir.join(format!("case{i}")), dir.join(format!("prev{i}")));
        generate_ok(prev, &prev_out, &[]);
        copy_dir(&model(version), &case);
        edit(&case.join(file), from, to);
        refused_against(&case.join("contract.toml"), &prev_out, expected);
    }
    let older = dir.join("older");
    generate_ok("v1_1", &older, &[]);
    refused_against(
        &model("v1_0").join("contract.toml"),
        &older,
        "older than the previous",
    );
    let _ = fs::remove_dir_all(&dir);
}

/// The contract.toml text the rejected-settings list is declared before.
const REJECTED_AT: &str = "[[shadow_constant]]\npath = \"state.connected\"";

/// @brief `REJECTED_AT` with the list declared before it: `fields` is its `path` line and any
/// further lines (`variants`).
/// v2: further lines.
/// @version 2
fn rejected_at(fields: &str) -> String {
    format!("[shadow_rejected]\n{fields}\n\n{REJECTED_AT}")
}

/// @brief Generate the `version` model (in `dir/tag`) with the list declared by `fields` (none:
/// no list) against the contract.json in `prev`; returns (success, stderr).
/// v2: the declaration's fields, and the case named by `tag`.
/// @version 2
fn against(dir: &Path, tag: &str, fields: Option<&str>, prev: &Path) -> (bool, String) {
    let case = dir.join(tag);
    let edits = fields.map(|f| ("contract.toml", REJECTED_AT, rejected_at(f)));
    let edits: Vec<(&str, &str, &str)> =
        edits.iter().map(|(f, a, b)| (*f, *a, b.as_str())).collect();
    edited_copy("v1_1", &case, &edits);
    let previous = prev.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    generate(&case.join("contract.toml"), &case.join("out"), &args)
}

/// @brief Generate the 1.0 model with the list declared by `fields` into `dir/prev/out`, for
/// sku_a; returns that output directory.
/// @version 1
fn previous_with(dir: &Path, fields: &str) -> std::path::PathBuf {
    let prev = dir.join("prev");
    edited_copy(
        "v1_0",
        &prev,
        &[("contract.toml", REJECTED_AT, &rejected_at(fields))],
    );
    let (ok, stderr) = generate(&prev.join("contract.toml"), &prev.join("out"), &[]);
    assert!(ok, "{stderr}");
    prev.join("out")
}

/// The refusal of a list dropped or moved in a minor.
const REJECTED_REMOVED: &str = "shadow_rejected state.rejected changed or was removed in a minor";

#[test]
fn the_rejected_settings_path_is_kept_in_a_minor_and_may_be_added() {
    let dir = unique_dir("previous_rejected");
    let prev = previous_with(&dir, "path = \"state.rejected\"");
    for (tag, fields) in [("none", None), ("moved", Some("path = \"state.refused\""))] {
        let (ok, stderr) = against(&dir, tag, fields, &prev);
        assert!(!ok && stderr.contains(REJECTED_REMOVED), "{tag}: {stderr}");
    }
    let (ok, stderr) = against(&dir, "kept", Some("path = \"state.rejected\""), &prev);
    assert!(ok, "kept: {stderr}");
    let plain = dir.join("plain");
    generate_ok("v1_0", &plain, &[]);
    let (ok, stderr) = against(&dir, "added", Some("path = \"state.added\""), &plain);
    assert!(ok, "added in a minor: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn the_rejected_settings_path_is_frozen_per_variant() {
    let dir = unique_dir("previous_rejected_variants");
    let prev = previous_with(&dir, "path = \"state.rejected\"\nvariants = [\"sku_a\"]");
    let widened = "path = \"state.rejected\"\nvariants = [\"sku_a\", \"sku_b\"]";
    let (ok, stderr) = against(&dir, "widened", Some(widened), &prev);
    assert!(ok, "another variant added in a minor: {stderr}");
    let elsewhere = "path = \"state.rejected\"\nvariants = [\"sku_b\"]";
    let (ok, stderr) = against(&dir, "elsewhere", Some(elsewhere), &prev);
    assert!(
        !ok && stderr.contains(REJECTED_REMOVED),
        "dropped on sku_a: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief The telemetry topic limited to `variants` (a TOML list).
/// @version 1
fn telemetry_on(variants: &str) -> String {
    format!("{TELEMETRY}\nvariants = {variants}")
}

/// @brief Generate the 1.1 model with the telemetry topic on `variants` against `prev`;
/// returns (success, stderr).
/// @version 1
fn telemetry_against(dir: &Path, tag: &str, variants: &str, prev: &Path) -> (bool, String) {
    let case = dir.join(tag);
    edited_copy(
        "v1_1",
        &case,
        &[("contract.toml", TELEMETRY, &telemetry_on(variants))],
    );
    let previous = prev.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    generate(&case.join("contract.toml"), &case.join("out"), &args)
}

#[test]
fn a_topic_is_frozen_per_variant() {
    let dir = unique_dir("previous_topic_variants");
    let prev = dir.join("prev");
    edited_copy(
        "v1_0",
        &prev,
        &[("contract.toml", TELEMETRY, &telemetry_on("[\"sku_a\"]"))],
    );
    let (ok, stderr) = generate(&prev.join("contract.toml"), &prev.join("out"), &[]);
    assert!(ok, "{stderr}");
    let prev = prev.join("out");
    let (ok, stderr) = telemetry_against(&dir, "widened", "[\"sku_a\", \"sku_b\"]", &prev);
    assert!(ok, "another variant added in a minor: {stderr}");
    let (ok, stderr) = telemetry_against(&dir, "elsewhere", "[\"sku_b\"]", &prev);
    let removed = "topic telemetry changed or was removed in a minor";
    assert!(
        !ok && stderr.contains(removed),
        "dropped on sku_a: {stderr}"
    );
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Placeholder sku of `max_len` and a new topic using it, declared before the telemetry
/// topic (which stays as it is).
/// @version 1
fn sku_placeholder(max_len: u16) -> String {
    format!("[[placeholders]]\nname = \"sku\"\nmax_len = {max_len}\n\n[[topics]]\nid = \"by_sku\"\ntemplate = \"{{stage}}/{{sku}}/by\"\ndirection = \"pub\"\n\n{TELEMETRY_TOPIC}")
}

/// @brief Generate `version` with the sku placeholder of `max_len` (none: unedited) into
/// `dir/tag`, against `prev` when given; returns (success, stderr).
/// @version 1
fn with_sku(
    dir: &Path,
    tag: &str,
    version: &str,
    max_len: Option<u16>,
    prev: Option<&Path>,
) -> (bool, String) {
    let case = dir.join(tag);
    let text = max_len.map(sku_placeholder);
    let edits: Vec<(&str, &str, &str)> = text
        .iter()
        .map(|t| ("contract.toml", TELEMETRY_TOPIC, t.as_str()))
        .collect();
    edited_copy(version, &case, &edits);
    let previous = prev.map(|p| p.join("out").join("contract.json"));
    let args: Vec<&str> = previous
        .iter()
        .flat_map(|p| ["--previous-contract", p.to_str().unwrap()])
        .collect();
    generate(&case.join("contract.toml"), &case.join("out"), &args)
}

#[test]
fn a_placeholder_is_kept_in_a_minor_and_may_be_added() {
    let dir = unique_dir("previous_placeholders");
    assert!(with_sku(&dir, "prev", "v1_0", Some(32), None).0);
    assert!(with_sku(&dir, "plain", "v1_0", None, None).0);
    let prev = dir.join("prev");
    let removed = "placeholder sku changed or was removed in a minor";
    for (tag, max_len) in [("dropped", None), ("changed", Some(16))] {
        let (ok, stderr) = with_sku(&dir, tag, "v1_1", max_len, Some(&prev));
        assert!(!ok && stderr.contains(removed), "{tag}: {stderr}");
    }
    let (ok, stderr) = with_sku(&dir, "kept", "v1_1", Some(32), Some(&prev));
    assert!(ok, "kept: {stderr}");
    let (ok, stderr) = with_sku(&dir, "added", "v1_1", Some(32), Some(&dir.join("plain")));
    assert!(ok, "added in a minor: {stderr}");
    let _ = fs::remove_dir_all(&dir);
}

/// New-principal grants of the 1.1 model: write on an MFG key and on both reset commands, read
/// on an MFG key, a secret and a diag class, and write on a secret and on a key new in 1.1 (and
/// its alias).
const OTA_GRANTS: &str = "  { namespace = \"drive\", class = \"settings\", access = \"r\" },\n  { namespace = \"core\", class = \"diag\", access = \"r\" },\n  { key = \"mfg.identity.serial\", access = \"w\" },\n  { key = \"mfg.identity.device_id\", access = \"r\" },\n  { key = \"core.command.factory_reset\", access = \"w\" },\n  { key = \"core.command.pre_reset\", access = \"w\" },\n  { key = \"net.credentials.wifi_psk\", access = \"w\" },\n  { key = \"drive.state.velocity\", access = \"w\" },\n  { key = \"drive.state.speed\", access = \"w\" },\n]";

/// Where the 1.1 model's app.ota grants end.
const OTA_GRANTS_END: &str =
    "  { namespace = \"drive\", class = \"settings\", access = \"r\" },\n]\n\n[[uid_caps]]";

/// @brief Generate the 1.1 model edited by `edits` (file, from, to) against 1.0; return the
/// release-review lines.
/// @version 1
fn review_lines(tag: &str, edits: &[(&str, &str, &str)]) -> Vec<String> {
    let dir = unique_dir(tag);
    let (prev, model_dir, gen) = (dir.join("prev"), dir.join("model"), dir.join("gen"));
    generate_ok("v1_0", &prev, &[]);
    edited_copy("v1_1", &model_dir, edits);
    let previous = prev.join("contract.json");
    let args = ["--previous-contract", previous.to_str().unwrap()];
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &gen, &args);
    assert!(ok, "{stderr}");
    let review = fs::read_to_string(gen.join("release-review.txt")).unwrap();
    let _ = fs::remove_dir_all(&dir);
    review.lines().map(str::to_string).collect()
}

#[test]
fn release_review_lists_only_sensitive_grants_of_new_principals() {
    let ota = format!("{OTA_GRANTS}\n\n[[uid_caps]]");
    let lines = review_lines("review", &[("contract.toml", OTA_GRANTS_END, &ota)]);
    assert_eq!(
        lines,
        [
            "new principal app.ota: w on existing core.command.factory_reset",
            "new principal app.ota: w on existing core.command.pre_reset",
            "new principal app.ota: w on existing mfg.identity.serial",
            "new principal app.ota: r on existing net.credentials.cloud_token",
        ]
    );
}

/// The 1.1 model's trace-enable command, moved off its role to an owner new in 1.1, and a new
/// command taking the role.
const TRACE_ROLE: &str = "owner = \"core\"\npayload_max = 118\ndedupe_ring = 4\nwk = \"TRACE_ENABLE\"\ndoc = \"Signed trace-enable envelope; the core verifies it.\"\n";
const TRACE_ROLE_MOVED: &str = "owner = \"app.ota\"\npayload_max = 118\ndedupe_ring = 4\ndoc = \"Signed trace-enable envelope; the core verifies it.\"\n\n[[classes.commands]]\nid = \"trace_enable2\"\ncmd_index = 9\nresult_index = 10\ndedupe_index = 11\nintroduced_in = \"1.1\"\npersistence = \"none\"\nowner = \"core\"\npayload_max = 118\ndedupe_ring = 4\nwk = \"TRACE_ENABLE\"\n";

#[test]
fn a_well_known_role_may_not_move_in_a_minor() {
    let cloud = "  { key = \"core.command.trace_enable\", access = \"rw\" },\n  { key = \"drive.command.map_edit\", access = \"rw\" },";
    let cloud2 = "  { key = \"core.command.trace_enable\", access = \"rw\" },\n  { key = \"core.command.trace_enable2\", access = \"rw\" },\n  { key = \"drive.command.map_edit\", access = \"rw\" },";
    let ota = "  { key = \"core.command.trace_enable\", access = \"rw\" },\n  { key = \"core.command.trace_enable_result\", access = \"rw\" },\n]\n\n[[uid_caps]]";
    let moved: &[(&str, &str, &str)] = &[
        ("ns/core.toml", TRACE_ROLE, TRACE_ROLE_MOVED),
        ("contract.toml", cloud, cloud2),
        ("contract.toml", "]\n\n[[uid_caps]]", ota),
    ];
    let dropped: &[(&str, &str, &str)] = &[("ns/core.toml", "wk = \"DIAG_FSYNC_COUNT\"\n", "")];
    let cases = [
        (moved, "well-known role TRACE_ENABLE moved from core.command.trace_enable to core.command.trace_enable2 in a minor"),
        (dropped, "well-known role DIAG_FSYNC_COUNT moved from core.diag.fsync_count to no key in a minor"),
    ];
    let dir = unique_dir("role_move");
    let prev = dir.join("prev");
    generate_ok("v1_0", &prev, &[]);
    for (i, (edits, expected)) in cases.iter().enumerate() {
        let case = dir.join(format!("model{i}"));
        edited_copy("v1_1", &case, edits);
        refused_against(&case.join("contract.toml"), &prev, expected);
    }
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Run contract mode on `model_dir/contract.toml` and assert it is refused with a log
/// containing `expected`, generating nothing.
/// @version 1
fn refused(model_dir: &Path, expected: &str) {
    let out = model_dir.join("out");
    let (ok, stderr) = generate(&model_dir.join("contract.toml"), &out, &[]);
    assert!(
        !ok && stderr.contains(expected),
        "'{expected}' in:\n{stderr}"
    );
    assert!(!out.join("dmt_tables.c").exists(), "nothing is generated");
}

#[test]
fn a_model_without_principals_is_refused() {
    let dir = unique_dir("no_principals");
    copy_dir(&model("v1_0"), &dir);
    let contract = dir.join("contract.toml");
    let text = fs::read_to_string(&contract).unwrap();
    let head = text.split("[[principals]]").next().unwrap();
    fs::write(&contract, head).unwrap();
    refused(&dir, "the model declares no principals");
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_namespace_directory_without_toml_files_is_refused() {
    let dir = unique_dir("empty_ns");
    copy_dir(&model("v1_0"), &dir);
    for entry in fs::read_dir(dir.join("ns")).unwrap() {
        fs::remove_file(entry.unwrap().path()).unwrap();
    }
    fs::write(dir.join("ns/README.md"), "not a namespace").unwrap();
    refused(&dir, "no namespace .toml files");
    let _ = fs::remove_dir_all(&dir);
}

/// Refusals that need more than one edit: model version, edits (file, from, to), expected log.
type MultiCase = (
    &'static str,
    &'static [(&'static str, &'static str, &'static str)],
    &'static str,
);

const MULTI_CASES: &[MultiCase] = &[
    // two (key, value) pairs that name one enum constant
    (
        "v1_0",
        &[
            ("ns/drive.toml", "eco = 1\n", "eco = 1\neco_off = 9\n"),
            (
                "ns/drive.toml",
                "[[classes.keys]]\nid = \"mode\"",
                "[[classes.keys]]\nid = \"mode_eco\"\nkey_index = 9\ntype = \"uint8\"\nenum = \"mode\"\nintroduced_in = \"1.0\"\nwrite_rate = \"1/min\"\ndefault = 1\n\n[[classes.keys]]\nid = \"mode\"",
            ),
        ],
        "drive.settings.mode_eco: enum constant DMT_E_DRIVE_SETTINGS_MODE_ECO_OFF is also drive.settings.mode's",
    ),
    // a command role moved onto a plain key
    (
        "v1_0",
        &[
            ("ns/core.toml", "wk = \"RESET_PRE_COMMAND\"\n", ""),
            ("ns/core.toml", "wk = \"DIAG_FSYNC_COUNT\"", "wk = \"RESET_PRE_COMMAND\""),
        ],
        "core.diag.fsync_count: role RESET_PRE_COMMAND needs a command key",
    ),
    // a typed role on a command key of the right type, persistence and survives-reset
    (
        "v1_0",
        &[
            ("ns/core.toml", "wk = \"DIAG_SUBSCRIBERS\"\n", ""),
            (
                "ns/drive.toml",
                "persistence = \"durable\"\nowner = \"app.map\"\npayload = \"struct\"",
                "persistence = \"none\"\nowner = \"app.map\"\npayload = \"binary\"\nwk = \"DIAG_SUBSCRIBERS\"",
            ),
        ],
        "drive.command.map_edit: role DIAG_SUBSCRIBERS needs a plain key, not a command",
    ),
    // The literal-name flag is only for a literal beside {shadow_name}
    (
        "v1_0",
        &[
            ("contract.toml", "shadow/name/{shadow_name}/update\"", "shadow/name/device/update\""),
            ("contract.toml", "shadow/name/{shadow_name}/update/delta\"", "shadow/name/device/update/delta\"\nliteral_shadow_name = true"),
        ],
        "topic shadow_delta: literal_shadow_name is for a literal shadow name beside {shadow_name}",
    ),
];

#[test]
fn every_multi_edit_refusal_has_a_triggering_model() {
    let dir = unique_dir("multi");
    for (i, (version, edits, expected)) in MULTI_CASES.iter().enumerate() {
        let case = dir.join(format!("case{i}"));
        edited_copy(version, &case, edits);
        refused(&case, expected);
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_model_declaring_variants_needs_a_declared_variant() {
    let dir = unique_dir("variant_flag");
    let contract = model("v1_0").join("contract.toml");
    let out = dir.join("out");
    let base = [
        "--contract",
        contract.to_str().unwrap(),
        "--output",
        out.to_str().unwrap(),
        "--allow-dirty-ingot",
    ];
    let (ok, stderr) = run_ingot(&[&base[..], &["--variant", "nosuch"]].concat());
    assert!(
        !ok && stderr.contains("--variant nosuch is not declared (declared: sku_a, sku_b)"),
        "{stderr}"
    );
    let (ok, stderr) = run_ingot(&base);
    assert!(
        !ok && stderr.contains("--variant must name one of: sku_a, sku_b"),
        "{stderr}"
    );
    assert!(!out.join("dmt_tables.c").exists());
    let _ = fs::remove_dir_all(&dir);
}

/// @brief Remove the fields ingot added after TIV 1.0 shipped (variants, enum_wire) from the
/// contract.json in `gen`, as an older ingot would have written it.
/// @version 1
fn strip_to_old_snapshot(gen: &Path) {
    let path = gen.join("contract.json");
    let mut json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
    json.as_object_mut().unwrap().remove("variants");
    for key in json["keys"].as_array_mut().unwrap() {
        key.as_object_mut().unwrap().remove("enum_wire");
    }
    fs::write(&path, serde_json::to_string_pretty(&json).unwrap()).unwrap();
}

#[test]
fn a_snapshot_from_before_wire_strings_is_still_checked() {
    // an old contract.json reads back, and enum changes are still refused through the enum
    // values alone: the enum dropped from a key, and a value removed
    let edits = [
        (
            "enum = \"mode\"\nintroduced_in = \"1.0\"",
            "introduced_in = \"1.0\"",
        ),
        ("eco = 1\n", ""),
    ];
    let dir = unique_dir("old_snapshot");
    for (i, (from, to)) in edits.iter().enumerate() {
        let (prev, case) = (dir.join(format!("prev{i}")), dir.join(format!("model{i}")));
        generate_ok("v1_0", &prev, &[]);
        strip_to_old_snapshot(&prev);
        edited_copy("v1_1", &case, &[("ns/drive.toml", from, to)]);
        refused_against(
            &case.join("contract.toml"),
            &prev,
            "drive.settings.mode: ID",
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn ranges_and_sets_may_only_widen_in_a_minor() {
    // against the 1.1 model itself: each edit of an existing key's limits, accepted or refused
    let accepted: &[(&str, &str)] = &[
        ("max = 2000\n", "max = 3000\n"),
        ("min = 0\nmax = 2000\nstep = 10\n", ""),
        (
            "allowed = [250, 500, 1000]",
            "allowed = [250, 500, 750, 1000]",
        ),
        (
            "allowed = [250, 500, 1000]",
            "min = 0\nmax = 1000\nstep = 250",
        ),
    ];
    let refused_edits: &[(&str, &str)] = &[
        ("allowed = [250, 500, 1000]", "allowed = [250, 500]"),
        (
            "allowed = [250, 500, 1000]",
            "min = 0\nmax = 750\nstep = 250",
        ),
        ("id = \"odometer\"", "id = \"odometer\"\nmin = 0\nmax = 10"),
        ("id = \"odometer\"", "id = \"odometer\"\nallowed = [0]"),
    ];
    let dir = unique_dir("widen");
    let prev = dir.join("prev");
    generate_ok("v1_1", &prev, &[]);
    let args_for = |p: &Path| p.join("contract.json");
    for (i, (from, to)) in accepted.iter().chain(refused_edits).enumerate() {
        let case = dir.join(format!("model{i}"));
        edited_copy("v1_1", &case, &[("ns/drive.toml", from, to)]);
        let previous = args_for(&prev);
        let out = case.join("out");
        let args = ["--previous-contract", previous.to_str().unwrap()];
        let (ok, stderr) = generate(&case.join("contract.toml"), &out, &args);
        let narrowed = stderr.contains("its range or allowed set narrowed");
        assert_eq!(
            (ok, narrowed),
            (i < accepted.len(), i >= accepted.len()),
            "case {i}: {stderr}"
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn an_unset_sentinel_default_lies_outside_the_range() {
    let dir = unique_dir("unset");
    let case = dir.join("model");
    edited_copy(
        "v1_0",
        &case,
        &[
            (
                "ns/drive.toml",
                "min = 0\nmax = 2000\nstep = 10\n",
                "min = 200\nmax = 2000\nstep = 10\nunset = 100\n",
            ),
            (
                "ns/drive.toml",
                "defaults = { sku_b = 80 }",
                "defaults = { sku_b = 200 }",
            ),
        ],
    );
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
    assert!(ok, "{stderr}");
    let keys = contract_json(&case.join("out"))["keys"].clone();
    let limit = keys
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["path"] == "drive.settings.speed_limit")
        .unwrap()
        .clone();
    assert_eq!(limit["unset"], 100);
    let _ = fs::remove_dir_all(&dir);
}

/// Edits of a model copy: (file, from, to).
type Edits = &'static [(&'static str, &'static str, &'static str)];

/// The speed-limit range of the generic 1.1 model, and the same range raised so its default 100
/// is an `unset` sentinel.
const LIMIT_RANGE: &str = "min = 0\nmax = 2000\nstep = 10\n";
const LIMIT_SENTINEL: &str = "min = 200\nmax = 2000\nstep = 10\nunset = 100\n";

/// A scaled speed limit, then its scale changed.
const SCALE_BEFORE: Edits = &[(
    "ns/drive.toml",
    LIMIT_RANGE,
    "min = 0\nmax = 2000\nstep = 10\nscale = 2.0\n",
)];
const SCALE_AFTER: Edits = &[(
    "ns/drive.toml",
    LIMIT_RANGE,
    "min = 0\nmax = 2000\nstep = 10\nscale = 4.0\n",
)];
/// A speed limit whose default 100 is its sentinel, then the sentinel (and default) changed.
const SKU_B_LIMIT: (&str, &str, &str) = (
    "ns/drive.toml",
    "defaults = { sku_b = 80 }",
    "defaults = { sku_b = 200 }",
);
const SENTINEL_BEFORE: Edits = &[("ns/drive.toml", LIMIT_RANGE, LIMIT_SENTINEL), SKU_B_LIMIT];
const SENTINEL_AFTER: Edits = &[
    (
        "ns/drive.toml",
        LIMIT_RANGE,
        "min = 200\nmax = 2000\nstep = 10\nunset = 150\n",
    ),
    ("ns/drive.toml", "default = 100", "default = 150"),
    SKU_B_LIMIT,
];

#[test]
fn scale_and_sentinel_are_kept_in_a_minor() {
    let dir = unique_dir("limits_kept");
    let cases = [
        (SCALE_BEFORE, SCALE_AFTER),
        (SENTINEL_BEFORE, SENTINEL_AFTER),
    ];
    for (i, (before, after)) in cases.iter().enumerate() {
        let (prev_model, prev, case) = (
            dir.join(format!("pm{i}")),
            dir.join(format!("prev{i}")),
            dir.join(format!("m{i}")),
        );
        edited_copy("v1_1", &prev_model, before);
        let (ok, stderr) = generate(&prev_model.join("contract.toml"), &prev, &[]);
        assert!(ok, "case {i} previous: {stderr}");
        edited_copy("v1_1", &case, after);
        refused_against(
            &case.join("contract.toml"),
            &prev,
            "unit, scale or unset changed",
        );
    }
    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn a_range_without_a_step_steps_by_one() {
    let dir = unique_dir("step_one");
    let case = dir.join("model");
    edited_copy("v1_0", &case, &[("ns/drive.toml", "step = 10\n", "")]);
    let (ok, stderr) = generate(&case.join("contract.toml"), &case.join("out"), &[]);
    assert!(ok, "{stderr}");
    let keys = contract_json(&case.join("out"))["keys"].clone();
    let limit = keys
        .as_array()
        .unwrap()
        .iter()
        .find(|k| k["path"] == "drive.settings.speed_limit")
        .unwrap()
        .clone();
    assert_eq!(limit["range"]["step"], 1);
    let _ = fs::remove_dir_all(&dir);
}
