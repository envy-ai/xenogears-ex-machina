/* Read-only field-load observation. Capture actual input before each original
 * decompression and its destination immediately afterward; never change game RAM. */
#include "common.h"
#include "resident/cd.h"
#include "resident/mode.h"
#include "../decomp/src/field/field_mode.h"

void xem_original_field_load_from_bundle(void);
void xem_original_field_decompress_component(s32 unused, void *source, void *destination);
void xem_host_field_source(void *record, u32 length);

static void xem_observe_field_source(u32 phase, void *source, void *destination) {
    u32 record[8];
    u32 map = (u32)mode_field_map_id & 0xFFF;
    u32 slot = (u32)cd_selected_directory + 0xB8 + map * 2 - 1;
    record[0] = phase;
    record[1] = (u32)mode_read_ahead_block;
    record[2] = (u32)mode_read_ahead_size;
    record[3] = map;
    record[4] = slot;
    record[5] = (u32)cd_file_index + slot * 7;
    record[6] = (u32)source;
    record[7] = (u32)destination;
    xem_host_field_source(record, sizeof(record));
}

void field_load_from_bundle(void) {
    xem_observe_field_source(0, 0, 0);
    xem_original_field_load_from_bundle();
    xem_observe_field_source(3, 0, 0);
}

void field_decompress_component(s32 unused, void *source, void *destination) {
    xem_observe_field_source(1, source, destination);
    xem_original_field_decompress_component(unused, source, destination);
    xem_observe_field_source(2, source, destination);
}
