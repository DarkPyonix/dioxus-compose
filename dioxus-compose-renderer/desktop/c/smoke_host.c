/*
 * Stand-in for the Rust Host: links the renderer library and runs it, the way
 * `dioxus_compose::launch` does with LoopMode::Renderer.
 *
 * It also implements the `dioxus_compose_host_*` half of the boundary, because the renderer
 * resolves those symbols from the executable it is loaded into and calls `host_init` as soon
 * as the window composes. The batch it returns is hand-encoded with the same fixed-layout
 * records the Rust Host emits, so the smoke test exercises the real decode path.
 */
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifdef _WIN32
#define HOST_EXPORT __declspec(dllexport)
#else
#define HOST_EXPORT
#endif

int32_t dioxus_compose_renderer_run(void);

typedef struct {
    const uint8_t *ptr;
    uint32_t len;
    int64_t result;
} MutationBatch;

enum { STATUS_OK = 0, STATUS_PROTOCOL_ERROR = -1 };

/* Mutation tags (schema.rs). */
enum { TAG_ENVELOPE = 0, TAG_CREATE = 1, TAG_SET_PROP = 2, TAG_INSERT = 4 };
/* WidgetKind tags. */
enum { WIDGET_COLUMN = 1, WIDGET_TEXT = 4, WIDGET_TEXT_FIELD = 5, WIDGET_BUTTON = 6 };
/* PropertyKind tags. */
enum { PROP_TEXT = 1, PROP_ON_CLICK = 5 };
/* PropertyValue tags. */
enum { VALUE_TEXT = 1, VALUE_INTEGER = 3 };

#define CLICK_HANDLER 7
#define LABEL_NODE 2

static uint8_t batch_bytes[512];
static uint32_t batch_length;
static uint32_t records_length;
static uint32_t record_count;

static void put_u16(uint32_t offset, uint16_t value) {
    memcpy(batch_bytes + offset, &value, sizeof value);
}

static void put_u32(uint32_t offset, uint32_t value) {
    memcpy(batch_bytes + offset, &value, sizeof value);
}

static void put_u64(uint32_t offset, uint64_t value) {
    memcpy(batch_bytes + offset, &value, sizeof value);
}

static void begin_batch(void) {
    memset(batch_bytes, 0, sizeof batch_bytes);
    records_length = 12;
    record_count = 0;
}

static uint32_t begin_record(uint16_t tag, uint16_t length) {
    uint32_t offset = records_length;
    put_u16(offset, tag);
    put_u16(offset + 2, length);
    records_length += length;
    record_count += 1;
    return offset;
}

/* Strings live after the records in the same buffer, addressed by (offset, length) pairs.
   Keeping them in the batch is what lets the renderer read them in place. */
static void put_string(uint32_t reference_offset, const char *text) {
    uint32_t length = (uint32_t)strlen(text);
    memcpy(batch_bytes + batch_length, text, length);
    put_u32(reference_offset, batch_length);
    put_u32(reference_offset + 4, length);
    batch_length += length;
}

static void create(uint32_t node_id, uint16_t widget) {
    uint32_t offset = begin_record(TAG_CREATE, 12);
    put_u32(offset + 4, node_id);
    put_u16(offset + 8, widget);
}

static void set_text_prop(uint32_t node_id, const char *text) {
    uint32_t offset = begin_record(TAG_SET_PROP, 24);
    put_u32(offset + 4, node_id);
    put_u16(offset + 8, PROP_TEXT);
    put_u16(offset + 10, VALUE_TEXT);
    /* Deferred until the record area is closed; recorded here, filled in end_batch. */
    put_u32(offset + 12, 0);
    put_u32(offset + 16, (uint32_t)strlen(text));
}

static void set_handler(uint32_t node_id, uint16_t property, uint64_t handler_id) {
    uint32_t offset = begin_record(TAG_SET_PROP, 24);
    put_u32(offset + 4, node_id);
    put_u16(offset + 8, property);
    put_u16(offset + 10, VALUE_INTEGER);
    put_u64(offset + 12, handler_id);
}

static void insert(uint32_t parent_id, uint32_t node_id, uint32_t index) {
    uint32_t offset = begin_record(TAG_INSERT, 16);
    put_u32(offset + 4, parent_id);
    put_u32(offset + 8, node_id);
    put_u32(offset + 12, index);
}

static void end_batch(void) {
    put_u16(0, TAG_ENVELOPE);
    put_u16(2, 12);
    put_u32(4, records_length);
    put_u32(8, record_count);
}

/*
 * Rebuilds the whole batch so that the text records point at string bytes placed after the
 * record area. Records are laid out first, then the strings are appended in order.
 */
#define IME_FIELD_TEXT "type Korean here"

/* DIOXUS_COMPOSE_SMOKE_IME=1 adds the text field on any platform. */
static int want_text_field(void) {
#ifdef __linux__
    return 1;
#else
    const char *flag = getenv("DIOXUS_COMPOSE_SMOKE_IME");
    return flag != NULL && flag[0] == '1' && flag[1] == '\0';
#endif
}

static void build_tree(const char *label) {
    begin_batch();
    create(1, WIDGET_COLUMN);
    create(LABEL_NODE, WIDGET_TEXT);
    uint32_t label_prop = records_length;
    set_text_prop(LABEL_NODE, label);
    insert(1, LABEL_NODE, 0);
    create(3, WIDGET_BUTTON);
    uint32_t button_prop = records_length;
    set_text_prop(3, "click me");
    set_handler(3, PROP_ON_CLICK, CLICK_HANDLER);
    insert(1, 3, 1);
    /* An editable control, for the manual IME checklist (typing Korean and watching the
       composition). It is off by default so the byte counts recorded for this batch stay
       stable, and because CI has nobody to type. Linux turns it on unconditionally: a real desktop run there is the only
       way to exercise XIM through ibus or fcitx. */
    uint32_t field_prop = 0;
    if (want_text_field()) {
        create(4, WIDGET_TEXT_FIELD);
        field_prop = records_length;
        set_text_prop(4, IME_FIELD_TEXT);
        insert(1, 4, 2);
    }
    end_batch();
    batch_length = records_length;
    put_string(label_prop + 12, label);
    put_string(button_prop + 12, "click me");
    if (field_prop != 0) {
        put_string(field_prop + 12, IME_FIELD_TEXT);
    }
}

static void build_label_update(const char *label) {
    begin_batch();
    uint32_t label_prop = records_length;
    set_text_prop(LABEL_NODE, label);
    end_batch();
    batch_length = records_length;
    put_string(label_prop + 12, label);
}

HOST_EXPORT int32_t dioxus_compose_host_init(
    const uint8_t *handshake, uint32_t len, MutationBatch *out
) {
    if (out == NULL || handshake == NULL || len < 12) {
        return STATUS_PROTOCOL_ERROR;
    }
    build_tree("smoke host: 0 clicks");
    out->ptr = batch_bytes;
    out->len = batch_length;
    out->result = 0;
    printf("dioxus_compose_host_init: %u bytes, %u records\n", batch_length, record_count);
    return STATUS_OK;
}

HOST_EXPORT int32_t dioxus_compose_host_dispatch_event(
    const uint8_t *event, uint32_t len, MutationBatch *out
) {
    static int clicks;
    static char label[64];
    if (out == NULL || event == NULL || len < 4) {
        return STATUS_PROTOCOL_ERROR;
    }
    clicks += 1;
    snprintf(label, sizeof label, "smoke host: %d clicks", clicks);
    build_label_update(label);
    printf("dioxus_compose_host_dispatch_event: click %d\n", clicks);
    out->ptr = batch_bytes;
    out->len = batch_length;
    out->result = 1;
    return STATUS_OK;
}

HOST_EXPORT int32_t dioxus_compose_host_render_frame(
    uint64_t frame_time_nanos, MutationBatch *out
) {
    (void)frame_time_nanos;
    if (out == NULL) {
        return STATUS_PROTOCOL_ERROR;
    }
    out->ptr = NULL;
    out->len = 0;
    out->result = 0;
    return STATUS_OK;
}

HOST_EXPORT void dioxus_compose_host_release_batch(MutationBatch *batch) {
    if (batch != NULL) {
        batch->ptr = NULL;
        batch->len = 0;
        batch->result = 0;
    }
}

HOST_EXPORT void dioxus_compose_host_shutdown(void) {
    printf("dioxus_compose_host_shutdown\n");
}

int main(void) {
    int32_t status = dioxus_compose_renderer_run();
    printf("dioxus_compose_renderer_run returned %d\n", status);
    return status == 0 ? 0 : 1;
}
