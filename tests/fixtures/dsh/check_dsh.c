/* Links against a generated dsh_tables.c and walks it, the way the library's binder, reporter,
 * command router and error reporter would: every table the header declares must be defined,
 * sorted where the interface says so, and every index and value reference must be in range.
 * Built by tests/shadow_tables.rs. */
#include <ctype.h>
#include <float.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#include "dsh_tables.h"
#include "dsh_internal_list.h"   /* written by the test: X(name) per DSH_INTERNAL_* define */

_Static_assert(DSH_IF_MAJOR == 1, "shadow table interface major");

/* Row counts of the tables a variant may leave empty, held as objects: a loop bounded by a 0u
 * constant is an always-false unsigned comparison under -Wextra. */
static const size_t n_desired = DSH_NUM_DESIRED;
static const size_t n_echo = DSH_NUM_ECHO;
static const size_t n_commands = DSH_NUM_COMMANDS;

/* The internal row constants dsh_tables.h states (minor 10), by name and value, then a NULL
 * terminator. */
typedef struct {
    const char *name;
    long        value;
} internal_constant_t;
#define X(n) { #n, (long)(n) },
static const internal_constant_t internal_constants[] = { DSH_INTERNAL_LIST { NULL, 0 } };
#undef X
static const size_t n_internal =
    sizeof internal_constants / sizeof internal_constants[0] - 1u;

/**
 * @brief   Whether a value reference lies inside dsh_value_blob.
 * @version 1
 * @return  1 when it does.
 */
static int value_ok(dsh_value_t v)
{
    return (size_t)v.off + v.len <= DSH_VALUE_BLOB_LEN;
}

/**
 * @brief   Whether `first .. first + count` lies inside a table of `total` rows.
 * @version 1
 * @return  1 when it does.
 */
static int slice_ok(size_t first, size_t count, size_t total)
{
    return first + count <= total;
}

/**
 * @brief   Whether a 16-bit index is DSH_NONE_U16 or below `total`.
 * @version 1
 * @return  1 when it is.
 */
static int index_ok(uint16_t i, size_t total)
{
    return i == DSH_NONE_U16 || i < total;
}

/**
 * @brief   Count violations in the enums: slices in range, outbound sorted by code and inbound
 *          by text, each strictly.
 * @version 1
 * @return  the number of violations.
 */
static int enum_violations(void)
{
    int bad = 0;
    size_t e, i;
    for (e = 0u; e < DSH_NUM_ENUMS; e++) {
        const dsh_enum_t *en = &dsh_enums[e];
        bad += !slice_ok(en->out_first, en->out_count, DSH_NUM_OUTBOUND);
        bad += !slice_ok(en->in_first, en->in_count, DSH_NUM_INBOUND);
        for (i = 1u; i < en->out_count; i++) {
            bad += dsh_outbound[en->out_first + i - 1u].code >= dsh_outbound[en->out_first + i].code;
        }
        for (i = 1u; i < en->in_count; i++) {
            bad += strcmp(dsh_inbound[en->in_first + i - 1u].text, dsh_inbound[en->in_first + i].text) >= 0;
        }
    }
    return bad;
}

/**
 * @brief   Whether `code` is an outbound code of enum `e`.
 * @version 1
 * @return  1 when it is.
 */
static int has_code(uint16_t e, int64_t code)
{
    size_t i;
    int found = 0;
    for (i = 0u; i < dsh_enums[e].out_count; i++) {
        found |= dsh_outbound[dsh_enums[e].out_first + i].code == code;
    }
    return found;
}

/**
 * @brief   Index of the desired row at `path` by binary search (as the binder does), or -1.
 * @version 1
 * @return  the row index, or -1.
 */
static long find_desired(const char *path)
{
    size_t lo = 0u;
    size_t hi = DSH_NUM_DESIRED;
    long found = -1;
    while (lo < hi && found < 0) {
        size_t mid = lo + (hi - lo) / 2u;
        int c = strcmp(dsh_desired[mid].path, path);
        if (c == 0) {
            found = (long)mid;
        } else if (c < 0) {
            lo = mid + 1u;
        } else {
            hi = mid;
        }
    }
    return found;
}

/**
 * @brief   Count violations in one desired row.
 * @version 1
 * @return  the number of violations.
 */
static int desired_row_violations(size_t i)
{
    const dsh_desired_t *d = &dsh_desired[i];
    int bad = find_desired(d->path) != (long)i;
    size_t k;
    bad += d->key == DMT_ID_NONE || !index_ok(d->enum_index, DSH_NUM_ENUMS);
    bad += !slice_ok(d->allowed_first, d->allowed_count, DSH_NUM_ALLOWED);
    bad += !slice_ok(d->implies_first, d->implies_count, DSH_NUM_IMPLIES);
    bad += d->apply_group != DSH_NONE_U8 && d->apply_group >= DSH_NUM_APPLY_GROUPS;
    bad += d->form != DSH_FORM_PLAIN && d->enum_index == DSH_NONE_U16;
    bad += d->inbound_unknown == DSH_IN_CODE && !has_code(d->enum_index, d->inbound_unknown_code);
    bad += d->inbound_unknown != DSH_IN_NONE && d->form != DSH_FORM_ENUM;
    for (k = 0u; k < d->implies_count && slice_ok(d->implies_first, k + 1u, DSH_NUM_IMPLIES); k++) {
        bad += !value_ok(dsh_implies[d->implies_first + k].value);
    }
    return bad;
}

/**
 * @brief   Count violations in the binder's tables: desired and echo rows sorted and unique.
 *          v2: either table may be empty.
 * @version 2
 * @return  the number of violations.
 */
static int binder_violations(void)
{
    int bad = 0;
    size_t i;
    for (i = 0u; i < n_desired; i++) {
        bad += i > 0u && strcmp(dsh_desired[i - 1u].path, dsh_desired[i].path) >= 0;
        bad += desired_row_violations(i);
    }
    for (i = 1u; i < n_echo; i++) {
        bad += strcmp(dsh_echo[i - 1u].path, dsh_echo[i].path) >= 0;
    }
    return bad;
}

/**
 * @brief   Number of leading object segments (each followed by a dot) two paths share.
 * @version 1
 * @return  the count.
 */
static size_t shared_objects(const char *a, const char *b)
{
    size_t n = 0u;
    size_t i = 0u;
    while (a[i] != '\0' && a[i] == b[i]) {
        n += a[i] == '.';
        i++;
    }
    return n;
}

/**
 * @brief   Count violations in one reported row's references.
 *          v2: the echo table may be empty.
 * @version 2
 * @return  the number of violations.
 */
static int reported_row_violations(const dsh_reported_t *r)
{
    int bad = r->scope == 0u || (r->scope & ~(DSH_SC_FULL | DSH_SC_DELTA)) != 0u;
    bad += !index_ok(r->enum_index, DSH_NUM_ENUMS);
    bad += r->kind == DSH_RK_KEY && r->key == DMT_ID_NONE;
    bad += r->kind == DSH_RK_CONSTANT && r->constant == NULL;
    bad += r->kind == DSH_RK_ECHO && r->ref >= n_echo;
    bad += r->outbound_unknown == DSH_OUT_WIRE && r->outbound_unknown_wire == NULL;
    bad += (r->flags & DSH_RF_FORCED) != 0u && (r->scope & DSH_SC_DELTA) == 0u;
    return bad;
}

/**
 * @brief   Count violations in the reporter's rows: references in range, and rows sharing an
 *          object prefix contiguous (a prefix left is never re-entered).
 * @version 1
 * @return  the number of violations.
 */
static int reporter_violations(void)
{
    int bad = 0;
    size_t i, j, k;
    for (i = 0u; i < DSH_NUM_REPORTED; i++) {
        bad += reported_row_violations(&dsh_reported[i]);
        for (j = i + 1u; j < DSH_NUM_REPORTED; j++) {
            for (k = j + 1u; k < DSH_NUM_REPORTED; k++) {
                size_t ik = shared_objects(dsh_reported[i].path, dsh_reported[k].path);
                bad += ik > shared_objects(dsh_reported[i].path, dsh_reported[j].path);
            }
        }
    }
    return bad;
}

/**
 * @brief   Count violations in the error groups: bits in range, sorted, below the width and
 *          never reserved.
 * @version 1
 * @return  the number of violations.
 */
static int error_violations(void)
{
    int bad = 0;
    size_t g, i;
    for (g = 0u; g < DSH_NUM_ERROR_GROUPS; g++) {
        const dsh_error_group_t *eg = &dsh_error_groups[g];
        bad += !slice_ok(eg->bits_first, eg->bits_count, DSH_NUM_ERROR_BITS);
        for (i = 0u; i < eg->bits_count; i++) {
            const dsh_error_bit_t *b = &dsh_error_bits[eg->bits_first + i];
            bad += b->bit >= eg->width || ((eg->reserved >> b->bit) & 1u) != 0u;
            bad += i > 0u && dsh_error_bits[eg->bits_first + i - 1u].bit >= b->bit;
        }
    }
    return bad;
}

/**
 * @brief   Count violations in one step's arguments.
 * @version 1
 * @return  the number of violations.
 */
static int step_violations(const dsh_step_t *s)
{
    int bad = !slice_ok(s->args_first, s->args_count, DSH_NUM_ARGS);
    size_t i;
    for (i = 0u; i < s->args_count && slice_ok(s->args_first, i + 1u, DSH_NUM_ARGS); i++) {
        const dsh_arg_t *a = &dsh_args[s->args_first + i];
        bad += !value_ok(a->value) || a->key == DMT_ID_NONE;
        bad += ((a->flags & DSH_AF_CONSTANT) != 0u) != (a->field == NULL);
        bad += !index_ok(a->enum_index, DSH_NUM_ENUMS);
    }
    return bad;
}

/**
 * @brief   Count violations in the command rows: a sub topic, steps exactly for the steps action.
 *          v2: the table may be empty.
 * @version 2
 * @return  the number of violations.
 */
static int command_violations(void)
{
    int bad = 0;
    size_t c, i;
    for (c = 0u; c < n_commands; c++) {
        const dsh_command_t *r = &dsh_commands[c];
        bad += r->topic >= DTP_NUM_TOPICS || dtp_topics[r->topic].direction != DTP_SUB;
        bad += !slice_ok(r->steps_first, r->steps_count, DSH_NUM_STEPS);
        bad += (r->action == DSH_DO_STEPS) != (r->steps_count > 0u);
        bad += r->action == DSH_DO_INTERNAL && r->internal == NULL;
        bad += r->body == DSH_BODY_OBJECT && r->body_object == NULL;
        for (i = 0u; i < r->steps_count && slice_ok(r->steps_first, i + 1u, DSH_NUM_STEPS); i++) {
            bad += step_violations(&dsh_steps[r->steps_first + i]);
        }
    }
    return bad;
}

/**
 * @brief   Count the vars of topic `p` without a name or with a bound outside 1..DTP_VAR_LEN_MAX.
 * @version 1
 * @return  the number of violations.
 */
static int var_violations(const dtp_topic_t *p)
{
    int bad = 0;
    size_t k;
    for (k = p->vars_first; k < (size_t)p->vars_first + p->vars_count; k++) {
        bad += dtp_vars[k].name == NULL || dtp_vars[k].max_len == 0u
               || dtp_vars[k].max_len > DTP_VAR_LEN_MAX;
    }
    return bad;
}

/**
 * @brief   Count violations in the topics: an ack names a pub topic, a source enum exists and
 *          holds its absent code, an envelope flag goes with an envelope field, the payload
 *          slice is in range and on a pub topic, the vars slice is in range, and every var has a
 *          name and a bound.
 * @version 3
 * @return  the number of violations.
 */
static int topic_violations(void)
{
    int bad = 0;
    size_t t;
    for (t = 0u; t < DTP_NUM_TOPICS; t++) {
        const dtp_topic_t *p = &dtp_topics[t];
        bad += p->ack != DSH_NONE_U8 && (p->ack >= DTP_NUM_TOPICS || dtp_topics[p->ack].direction != DTP_PUB);
        bad += !index_ok(p->source_enum, DSH_NUM_ENUMS);
        bad += p->source_enum != DSH_NONE_U16 && !has_code(p->source_enum, p->source_absent);
        bad += (p->flags & DTP_F_ID_REQUIRED) != 0u && p->id_field == NULL;
        bad += !slice_ok(p->payload_first, p->payload_count, DTP_NUM_PAYLOAD);
        bad += p->payload_count > 0u && p->direction != DTP_PUB;
        bad += !slice_ok(p->vars_first, p->vars_count, DTP_NUM_VARS) || p->vars_count > DTP_VARS_MAX;
        bad += slice_ok(p->vars_first, p->vars_count, DTP_NUM_VARS) && var_violations(p);
    }
    return bad;
}

/**
 * @brief   Bytes of the longer of `n` and `path` with its terminator.
 * @version 1
 * @return  the larger byte count.
 */
static size_t longer(size_t n, const char *path)
{
    size_t len = strlen(path) + 1u;
    return len > n ? len : n;
}

/**
 * @brief   Count binder bounds (minor 8) that differ from a walk of the binder and reporter
 *          tables: rejected-list members and the longest desired or echo path.
 *          v2: either table may be empty.
 * @version 2
 * @return  the number of violations.
 */
static int binder_bound_violations(void)
{
    size_t i, path = 0u, items = 0u;
    for (i = 0u; i < n_desired; i++) {
        path = longer(path, dsh_desired[i].path);
    }
    for (i = 0u; i < n_echo; i++) {
        path = longer(path, dsh_echo[i].path);
    }
    for (i = 0u; i < DSH_NUM_REPORTED; i++) {
        items = dsh_reported[i].kind == DSH_RK_REJECTED ? DSH_NUM_DESIRED + DSH_NUM_ECHO : items;
    }
    return (DSH_DESIRED_PATH_MAX_BYTES != path) + (DSH_REJECTED_MAX_ITEMS != items);
}

/**
 * @brief   Count error bounds (minor 8) that differ from a walk of the error groups: the ids the
 *          full table publishes and the largest id a published or declared bit carries.
 * @version 1
 * @return  the number of violations.
 */
static int error_bound_violations(void)
{
    size_t g, i, published = 0u;
    uint32_t top = 0u;
    for (g = 0u; g < DSH_NUM_ERROR_GROUPS; g++) {
        const dsh_error_group_t *eg = &dsh_error_groups[g];
        for (i = 0u; i <= eg->publish_max_bit; i++) {
            int out = ((eg->reserved >> i) & 1u) == 0u;
            published += (size_t)out;
            top = out && eg->wire_base + i > top ? eg->wire_base + (uint32_t)i : top;
        }
        for (i = 0u; i < eg->bits_count; i++) {
            uint32_t id = eg->wire_base + dsh_error_bits[eg->bits_first + i].bit;
            top = id > top ? id : top;
        }
    }
    return (DSH_ERROR_IDS_PUBLISHED != published) + (DSH_ERROR_ID_MAX != top);
}

/**
 * @brief   Bytes of the decimal text of `v`.
 * @version 1
 * @return  the byte count.
 */
static size_t decimal_len(long long v)
{
    return (size_t)snprintf(NULL, 0u, "%lld", v);
}

/**
 * @brief   The longest JSON text of a value of DMT_T_* `type`: "false", the longer extreme of an
 *          integer type, printf %f of -FLT_MAX, or `max_size` bytes each escaped as \u00XX in
 *          quotes; 0 for a type without a JSON value.
 * @version 1
 * @return  the byte count.
 */
static size_t type_text(uint8_t type, uint32_t max_size)
{
    static const long long lo[] = { 0, 0, 0, 0, INT8_MIN, INT16_MIN, INT32_MIN };
    static const long long hi[] = { 1, UINT8_MAX, UINT16_MAX, UINT32_MAX, INT8_MAX, INT16_MAX, INT32_MAX };
    size_t n = 0u;
    if (type == DMT_T_BOOL) {
        n = 5u;
    } else if (type == DMT_T_F32) {
        n = (size_t)snprintf(NULL, 0u, "%f", (double)-FLT_MAX);
    } else if (type == DMT_T_STRING) {
        n = 2u + 6u * (size_t)max_size;
    } else if (type <= DMT_T_I32) {
        size_t a = decimal_len(lo[type]), b = decimal_len(hi[type]);
        n = a > b ? a : b;
    }
    return n;
}

/**
 * @brief   The longest value text desired row `r` accepts: on an enum wire-string row its longest
 *          inbound string as a JSON string, else its type's.
 * @version 1
 * @return  the byte count.
 */
static size_t desired_text(const dsh_desired_t *r)
{
    size_t i, longest = 0u;
    if (r->form != DSH_FORM_ENUM) {
        return type_text(r->type, r->max_size);
    }
    for (i = 0u; i < dsh_enums[r->enum_index].in_count; i++) {
        size_t len = strlen(dsh_inbound[dsh_enums[r->enum_index].in_first + i].text);
        longest = len > longest ? len : longest;
    }
    return type_text(DMT_T_STRING, (uint32_t)longest);
}

/**
 * @brief   Count a desired value bound (minor 9) that differs from a walk of the binder tables:
 *          1 + the longest value text of a desired or echo row, 0 with none.
 * @version 1
 * @return  the number of violations.
 */
static int value_bound_violations(void)
{
    static const uint8_t echo_type[] = { DMT_T_BOOL, DMT_T_I32, DMT_T_F32, DMT_T_STRING };
    size_t i, top = 0u;
    for (i = 0u; i < n_desired; i++) {
        size_t n = desired_text(&dsh_desired[i]) + 1u;
        top = n > top ? n : top;
    }
    for (i = 0u; i < n_echo; i++) {
        size_t n = type_text(echo_type[dsh_echo[i].type], dsh_echo[i].max_size) + 1u;
        top = n > top ? n : top;
    }
    return DSH_DESIRED_VALUE_MAX_BYTES != top;
}

/**
 * @brief   The constant name of command row `r` into `out`: DSH_INTERNAL_ and the text that
 *          selects the row (wire or element, else field, of the body object's selector for an
 *          object body), upper case, each dot an underscore.
 * @version 1
 */
static void internal_name(const dsh_command_t *r, char *out, size_t size)
{
    static const char prefix[] = "DSH_INTERNAL_";
    const dsh_selector_t *s = r->body == DSH_BODY_OBJECT ? &r->body_selector : &r->selector;
    const char *verb = s->kind == DSH_SEL_TEXT || s->kind == DSH_SEL_ELEMENT ? s->wire : s->field;
    size_t i;
    (void)snprintf(out, size, "%s%s", prefix, verb);
    for (i = sizeof prefix - 1u; out[i] != '\0'; i++) {
        out[i] = out[i] == '.' ? '_' : (char)toupper((unsigned char)out[i]);
    }
}

/**
 * @brief   The internal row constant named `name`, or NULL.
 * @version 1
 * @return  the constant.
 */
static const internal_constant_t *internal_constant(const char *name)
{
    size_t i;
    const internal_constant_t *found = NULL;
    for (i = 0u; i < n_internal && found == NULL; i++) {
        found = strcmp(internal_constants[i].name, name) == 0 ? &internal_constants[i] : NULL;
    }
    return found;
}

/**
 * @brief   The number of command rows with an internal_id whose constant name is `name`.
 * @version 1
 * @return  the row count.
 */
static size_t internal_rows(const char *name)
{
    char mine[128];
    size_t c, n = 0u;
    for (c = 0u; c < n_commands; c++) {
        internal_name(&dsh_commands[c], mine, sizeof mine);
        n += dsh_commands[c].internal_id != 0u && strcmp(mine, name) == 0;
    }
    return n;
}

/**
 * @brief   Count internal row constants (minor 10) that differ from a walk of the command rows:
 *          each row with an internal_id is an internal row and has its constant at that id, and
 *          each constant names at least one such row.
 * @version 1
 * @return  the number of violations.
 */
static int internal_violations(void)
{
    char name[128];
    int bad = 0;
    size_t c, i;
    for (c = 0u; c < n_commands; c++) {
        const dsh_command_t *r = &dsh_commands[c];
        const internal_constant_t *k;
        internal_name(r, name, sizeof name);
        k = internal_constant(name);
        bad += r->internal_id != 0u &&
               (r->action != DSH_DO_INTERNAL || k == NULL || k->value != (long)r->internal_id);
    }
    for (i = 0u; i < n_internal; i++) {
        bad += internal_rows(internal_constants[i].name) == 0u;
    }
    return bad;
}

/**
 * @brief   Walk every table.
 *          v2: the consumer bounds (minor 8).
 *          v3: the desired value bound (minor 9).
 *          v4: the internal row constants (minor 10).
 * @version 4
 * @return  0 when the tables are consistent.
 */
int main(void)
{
    int bad = enum_violations() + binder_violations() + reporter_violations();
    bad += error_violations() + command_violations() + topic_violations();
    bad += binder_bound_violations() + error_bound_violations() + value_bound_violations();
    bad += internal_violations();
    bad += dsh_if_major_1 != 1u;
    return bad != 0;
}
