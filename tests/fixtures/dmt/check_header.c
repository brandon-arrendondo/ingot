/* Round-trips the struct value header that tests/struct_header.rs declares on drive.map.map_data
 * (a uint8, b int8, c uint16, d int16, e uint32, f int32, g bool) through the generated
 * dmt_constants.h: the constants, a hand-derived little-endian test vector for encode and
 * decode, the extremes of every signed type, and the refusals of a short value and a bool byte
 * of 2 (the struct untouched). Exit status: the number of failed checks. */
#include <stdint.h>
#include <string.h>

#include "dmt_constants.h"

static int bad;

#define EXPECT(cond) do { if (!(cond)) { bad++; } } while (0)

/** The wire image of `vector()`: AB, FE, 1234, FED4 (-300), DEADBEEF, F8A432EB (-123456789), 1. */
static const uint8_t WIRE[15] = {
    0xABu, 0xFEu, 0x34u, 0x12u, 0xD4u, 0xFEu, 0xEFu, 0xBEu,
    0xADu, 0xDEu, 0xEBu, 0x32u, 0xA4u, 0xF8u, 0x01u,
};

/**
 * @brief   The test vector, one value per field type.
 * @version 1
 * @return  the header
 */
static dmt_drive_map_map_data_header_t vector(void)
{
    dmt_drive_map_map_data_header_t h;
    h.a = 0xABu;
    h.b = -2;
    h.c = 0x1234u;
    h.d = -300;
    h.e = 0xDEADBEEFu;
    h.f = -123456789;
    h.g = true;
    return h;
}

/**
 * @brief   Whether two headers hold the same values.
 * @version 1
 * @return  1 when equal
 */
static int same(const dmt_drive_map_map_data_header_t *x, const dmt_drive_map_map_data_header_t *y)
{
    return x->a == y->a && x->b == y->b && x->c == y->c && x->d == y->d && x->e == y->e &&
           x->f == y->f && x->g == y->g;
}

/**
 * @brief   The constants: offsets packed in declaration order, 15 header bytes, the payload
 *          maximum max_size - 15.
 * @version 1
 */
static void constants(void)
{
    EXPECT(DMT_DRIVE_MAP_MAP_DATA_HEADER_BYTES == 15u);
    EXPECT(DMT_DRIVE_MAP_MAP_DATA_A_OFFSET == 0u && DMT_DRIVE_MAP_MAP_DATA_B_OFFSET == 1u);
    EXPECT(DMT_DRIVE_MAP_MAP_DATA_C_OFFSET == 2u && DMT_DRIVE_MAP_MAP_DATA_D_OFFSET == 4u);
    EXPECT(DMT_DRIVE_MAP_MAP_DATA_E_OFFSET == 6u && DMT_DRIVE_MAP_MAP_DATA_F_OFFSET == 10u);
    EXPECT(DMT_DRIVE_MAP_MAP_DATA_G_OFFSET == 14u);
    EXPECT(DMT_DRIVE_MAP_MAP_DATA_PAYLOAD_MAX == 65536u - 15u);
}

/**
 * @brief   The vector encodes to WIRE (and nothing past it) and WIRE decodes to the vector.
 * @version 1
 */
static void vector_round_trips(void)
{
    dmt_drive_map_map_data_header_t h = vector();
    dmt_drive_map_map_data_header_t back;
    uint8_t out[16];
    memset(out, 0x5A, sizeof out);
    dmt_drive_map_map_data_header_encode(&h, out);
    EXPECT(memcmp(out, WIRE, sizeof WIRE) == 0 && out[15] == 0x5Au);
    memset(&back, 0, sizeof back);
    EXPECT(dmt_drive_map_map_data_header_decode(WIRE, (uint32_t)sizeof WIRE, &back) == 0);
    EXPECT(same(&h, &back));
}

/**
 * @brief   The extremes of every signed and unsigned type survive encode and decode.
 * @version 1
 */
static void extremes_round_trip(void)
{
    dmt_drive_map_map_data_header_t lo = vector();
    dmt_drive_map_map_data_header_t hi = vector();
    dmt_drive_map_map_data_header_t back;
    uint8_t buf[15];
    lo.a = 0u;  lo.b = INT8_MIN;  lo.c = 0u;          lo.d = INT16_MIN;  lo.e = 0u;          lo.f = INT32_MIN;  lo.g = false;
    hi.a = 255u; hi.b = INT8_MAX; hi.c = UINT16_MAX;  hi.d = INT16_MAX;  hi.e = UINT32_MAX;  hi.f = INT32_MAX;  hi.g = true;
    dmt_drive_map_map_data_header_encode(&lo, buf);
    EXPECT(buf[1] == 0x80u && buf[5] == 0x80u && buf[13] == 0x80u && buf[14] == 0u);
    EXPECT(dmt_drive_map_map_data_header_decode(buf, 15u, &back) == 0 && same(&lo, &back));
    dmt_drive_map_map_data_header_encode(&hi, buf);
    EXPECT(buf[1] == 0x7Fu && buf[5] == 0x7Fu && buf[13] == 0x7Fu && buf[9] == 0xFFu);
    EXPECT(dmt_drive_map_map_data_header_decode(buf, 15u, &back) == 0 && same(&hi, &back));
}

/**
 * @brief   A value shorter than the header, and a bool byte of 2, are refused with the struct
 *          untouched; a value longer than the header decodes its header.
 * @version 1
 */
static void refusals(void)
{
    dmt_drive_map_map_data_header_t want = vector();
    dmt_drive_map_map_data_header_t h = want;
    uint8_t two[16];
    EXPECT(dmt_drive_map_map_data_header_decode(WIRE, 14u, &h) == -1 && same(&h, &want));
    memcpy(two, WIRE, sizeof WIRE);
    two[14] = 2u;
    two[15] = 0x77u;
    memset(&h, 0, sizeof h);
    EXPECT(dmt_drive_map_map_data_header_decode(two, 16u, &h) == -1 && h.a == 0u && h.f == 0);
    two[14] = 0u;
    EXPECT(dmt_drive_map_map_data_header_decode(two, 16u, &h) == 0 && !h.g && h.f == want.f);
}

/**
 * @brief   Run every check.
 * @version 1
 * @return  the number of failed checks
 */
int main(void)
{
    constants();
    vector_round_trips();
    extremes_round_trip();
    refusals();
    return bad;
}
