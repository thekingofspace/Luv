#include <math.h>
#include <stdbool.h>
#include <stddef.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <wchar.h>
#include "luv.h"

#ifdef _WIN32
#include <windows.h>
#else
#include <errno.h>
#include <pthread.h>
#include <unistd.h>
#endif

typedef struct Vec2 {
    float x;
    float y;
} Vec2;

typedef struct Record {
    int32_t id;
    double weight;
    char tag[8];
} Record;

typedef struct HookState {
    int32_t calls;
    int32_t vulkan;
    int32_t missing;
    float width;
} HookState;

LUV_EXPORT int32_t counter = 7;

LUV_EXPORT int32_t add(int32_t a, int32_t b) {
    return a + b;
}

LUV_EXPORT int32_t get_counter(void) {
    return counter;
}

LUV_EXPORT double mix(double value, float factor, int8_t offset, uint64_t big) {
    return value * factor + offset + (double)big;
}

LUV_EXPORT bool is_even(uint32_t value) {
    return value % 2 == 0;
}

LUV_EXPORT int16_t negate16(int16_t value) {
    return (int16_t)-value;
}

LUV_EXPORT size_t text_length(const char* text) {
    return text ? strlen(text) : 0;
}

LUV_EXPORT const char* greeting(void) {
    return "hello from C";
}

LUV_EXPORT const char* nothing(void) {
    return NULL;
}

LUV_EXPORT size_t wide_length(const wchar_t* text) {
    return wcslen(text);
}

LUV_EXPORT const wchar_t* wide_greeting(void) {
    return L"wide hello";
}

LUV_EXPORT void fill(uint8_t* data, size_t length, uint8_t value) {
    memset(data, value, length);
}

LUV_EXPORT int64_t sum_bytes(const uint8_t* data, size_t length) {
    int64_t total = 0;
    for (size_t index = 0; index < length; index++) {
        total += data[index];
    }
    return total;
}

LUV_EXPORT Vec2 vec2_add(Vec2 a, Vec2 b) {
    Vec2 result = {a.x + b.x, a.y + b.y};
    return result;
}

LUV_EXPORT float vec2_length(Vec2 value) {
    return sqrtf(value.x * value.x + value.y * value.y);
}

LUV_EXPORT Record make_record(int32_t id, double weight) {
    Record record;
    memset(&record, 0, sizeof record);
    record.id = id;
    record.weight = weight;
    memcpy(record.tag, "rec", 4);
    return record;
}

LUV_EXPORT int32_t record_id(const Record* record) {
    return record->id * 2;
}

LUV_EXPORT void set_out(int32_t* out, int32_t value) {
    *out = value;
}

LUV_EXPORT int32_t apply(int32_t (*callback)(int32_t), int32_t value) {
    return callback(value) + 1;
}

LUV_EXPORT int32_t reduce(const int32_t* values, size_t count, int32_t (*combine)(int32_t, int32_t)) {
    int32_t total = values[0];
    for (size_t index = 1; index < count; index++) {
        total = combine(total, values[index]);
    }
    return total;
}

LUV_EXPORT int32_t measure(int32_t (*callback)(const char*), const char* text) {
    return callback(text);
}

LUV_EXPORT double average(Vec2 (*callback)(Vec2), Vec2 value) {
    Vec2 result = callback(value);
    return (result.x + result.y) / 2.0;
}

typedef struct Job {
    int32_t (*callback)(int32_t);
    int32_t value;
    int32_t result;
} Job;

#ifdef _WIN32
static DWORD WINAPI run_job(LPVOID argument) {
    Job* job = (Job*)argument;
    job->result = job->callback(job->value);
    return 0;
}
#else
static void* run_job(void* argument) {
    Job* job = (Job*)argument;
    job->result = job->callback(job->value);
    return NULL;
}
#endif

LUV_EXPORT int32_t call_from_thread(int32_t (*callback)(int32_t), int32_t value) {
    Job job = {callback, value, 0};
#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, run_job, &job, 0, NULL);
    WaitForSingleObject(thread, INFINITE);
    CloseHandle(thread);
#else
    pthread_t thread;
    pthread_create(&thread, NULL, run_job, &job);
    pthread_join(thread, NULL);
#endif
    return job.result;
}

LUV_EXPORT void set_error(int32_t code) {
#ifdef _WIN32
    SetLastError((DWORD)code);
#else
    errno = code;
#endif
}

LUV_EXPORT uint64_t thread_id(void) {
#ifdef _WIN32
    return (uint64_t)GetCurrentThreadId();
#else
    return (uint64_t)pthread_self();
#endif
}

LUV_EXPORT void sleep_ms(uint32_t milliseconds) {
#ifdef _WIN32
    Sleep(milliseconds);
#else
    usleep(milliseconds * 1000);
#endif
}

LUV_EXPORT void render_hook(LuvRenderContext* context) {
    HookState* state = (HookState*)context->user_data;
    float tint[4] = {1.0f, 0.0f, 1.0f, 1.0f};
    unsigned char pattern[16] = {255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255, 255, 255, 255, 255};
    context->write_data(context, "tint", 0, tint, sizeof tint);
    context->write_texture(context, "pattern", 2, 2, pattern);
    context->set_draw_counts(context, 6, 1);
    if (state) {
        state->calls += 1;
        state->width = context->width;
        state->missing = context->write_data(context, "missing", 0, tint, sizeof tint);
        if (context->vulkan && context->vulkan->instance && context->vulkan->device && context->vulkan->get_instance_proc_addr) {
            void* found = context->vulkan->get_instance_proc_addr(context->vulkan->instance, "vkGetDeviceProcAddr");
            state->vulkan = found != NULL;
        }
    }
}

typedef struct Vec3 {
    double x;
    double y;
    double z;
} Vec3;

typedef struct Counter {
    int32_t count;
} Counter;

typedef struct Ticker {
    LuvRef* handler;
    int32_t count;
} Ticker;

static const LuvApi* api;
static const LuvClass* vec3_class;
static const LuvClass* counter_class;

LUV_EXPORT int32_t destroyed_counters = 0;

LUV_EXPORT double vec3_x_raw(const Vec3* value) {
    return value->x;
}

static Vec3* push_vec3(LuvCall* call, double x, double y, double z) {
    Vec3* value = (Vec3*)api->push_object(call, vec3_class);
    if (value) {
        value->x = x;
        value->y = y;
        value->z = z;
    }
    return value;
}

static void vec3_new(LuvCall* call) {
    push_vec3(call, api->opt_number(call, 0, 0), api->opt_number(call, 1, 0), api->opt_number(call, 2, 0));
}

static void vec3_from_udim(LuvCall* call) {
    double values[3];
    if (api->check_udim(call, 0, values)) {
        push_vec3(call, values[0], values[1], values[2]);
    }
}

static void vec3_zero(LuvCall* call) {
    push_vec3(call, 0, 0, 0);
}

static void vec3_get_x(LuvCall* call) {
    api->push_number(call, ((Vec3*)api->self_data(call))->x);
}

static void vec3_get_y(LuvCall* call) {
    api->push_number(call, ((Vec3*)api->self_data(call))->y);
}

static void vec3_get_z(LuvCall* call) {
    api->push_number(call, ((Vec3*)api->self_data(call))->z);
}

static void vec3_set_x(LuvCall* call) {
    ((Vec3*)api->self_data(call))->x = api->check_number(call, 0);
}

static void vec3_set_y(LuvCall* call) {
    ((Vec3*)api->self_data(call))->y = api->check_number(call, 0);
}

static void vec3_set_z(LuvCall* call) {
    ((Vec3*)api->self_data(call))->z = api->check_number(call, 0);
}

static void vec3_magnitude(LuvCall* call) {
    Vec3* self = (Vec3*)api->self_data(call);
    api->push_number(call, sqrt(self->x * self->x + self->y * self->y + self->z * self->z));
}

static void vec3_dot(LuvCall* call) {
    Vec3* self = (Vec3*)api->self_data(call);
    Vec3* other = (Vec3*)api->check_object(call, 0, vec3_class);
    if (other) {
        api->push_number(call, self->x * other->x + self->y * other->y + self->z * other->z);
    }
}

static void vec3_to_udim(LuvCall* call) {
    Vec3* self = (Vec3*)api->self_data(call);
    api->push_udim(call, self->x, self->y, self->z);
}

static void vec3_bump(LuvCall* call) {
    Vec3* self = (Vec3*)api->self_data(call);
    self->x += 1;
    api->push_self(call);
}

static uint64_t current_thread(void) {
#ifdef _WIN32
    return (uint64_t)GetCurrentThreadId();
#else
    return (uint64_t)pthread_self();
#endif
}

static void pause_ms(uint32_t milliseconds) {
#ifdef _WIN32
    Sleep(milliseconds);
#else
    usleep(milliseconds * 1000);
#endif
}

static void vec3_scaled_slowly(LuvCall* call) {
    Vec3* self = (Vec3*)api->self_data(call);
    double factor = api->check_number(call, 0);
    pause_ms(50);
    push_vec3(call, self->x * factor, self->y * factor, self->z * factor);
    api->push_number(call, (double)current_thread());
}

static void vec3_add(LuvCall* call) {
    Vec3* left = (Vec3*)api->check_object(call, 0, vec3_class);
    Vec3* right = (Vec3*)api->check_object(call, 1, vec3_class);
    if (left && right) {
        push_vec3(call, left->x + right->x, left->y + right->y, left->z + right->z);
    }
}

static void vec3_mul(LuvCall* call) {
    Vec3* value = (Vec3*)api->to_object(call, 0, vec3_class);
    double factor;
    if (value) {
        factor = api->check_number(call, 1);
    } else {
        factor = api->check_number(call, 0);
        value = (Vec3*)api->check_object(call, 1, vec3_class);
    }
    if (value) {
        push_vec3(call, value->x * factor, value->y * factor, value->z * factor);
    }
}

static void vec3_unm(LuvCall* call) {
    Vec3* value = (Vec3*)api->check_object(call, 0, vec3_class);
    if (value) {
        push_vec3(call, -value->x, -value->y, -value->z);
    }
}

static void vec3_eq(LuvCall* call) {
    Vec3* left = (Vec3*)api->to_object(call, 0, vec3_class);
    Vec3* right = (Vec3*)api->to_object(call, 1, vec3_class);
    api->push_boolean(call, left && right && left->x == right->x && left->y == right->y && left->z == right->z);
}

static void vec3_tostring(LuvCall* call) {
    Vec3* value = (Vec3*)api->check_object(call, 0, vec3_class);
    char text[128];
    if (value) {
        snprintf(text, sizeof text, "Vec3(%g, %g, %g)", value->x, value->y, value->z);
        api->push_string(call, text);
    }
}

static void vec3_len(LuvCall* call) {
    api->push_number(call, 3);
}

static const LuvMethod vec3_methods[] = {
    {"Dot", vec3_dot, LUV_INLINE},
    {"ToUDim", vec3_to_udim, LUV_INLINE},
    {"Bump", vec3_bump, LUV_INLINE},
    {"ScaledSlowly", vec3_scaled_slowly, LUV_WORKER},
    {"__add", vec3_add, 0},
    {"__mul", vec3_mul, 0},
    {"__unm", vec3_unm, 0},
    {"__eq", vec3_eq, 0},
    {"__tostring", vec3_tostring, 0},
    {"__len", vec3_len, 0},
    {0},
};

static const LuvProperty vec3_properties[] = {
    {"X", vec3_get_x, vec3_set_x},
    {"Y", vec3_get_y, vec3_set_y},
    {"Z", vec3_get_z, vec3_set_z},
    {"Magnitude", vec3_magnitude, NULL},
    {0},
};

static const LuvMethod vec3_statics[] = {
    {"new", vec3_new, LUV_INLINE},
    {"fromUDim", vec3_from_udim, LUV_INLINE},
    {0},
};

static const LuvProperty vec3_static_properties[] = {
    {"zero", vec3_zero, NULL},
    {0},
};

static void counter_destroy(void* data) {
    (void)data;
    destroyed_counters += 1;
}

static void counter_new(LuvCall* call) {
    Counter* counter = (Counter*)api->push_object(call, counter_class);
    if (counter) {
        counter->count = (int32_t)api->opt_number(call, 0, 0);
    }
}

static void counter_increment(LuvCall* call) {
    Counter* counter = (Counter*)api->self_data(call);
    counter->count += 1;
    api->push_number(call, counter->count);
}

static void counter_get(LuvCall* call) {
    api->push_number(call, ((Counter*)api->self_data(call))->count);
}

static const LuvMethod counter_methods[] = {
    {"Increment", counter_increment, LUV_PARALLEL},
    {0},
};

static const LuvProperty counter_properties[] = {
    {"Count", counter_get, NULL},
    {0},
};

static const LuvMethod counter_statics[] = {
    {"new", counter_new, LUV_INLINE},
    {0},
};

static void export_version(LuvCall* call) {
    api->push_string(call, "fixture");
    api->push_number(call, api->version);
}

static void export_inline_thread(LuvCall* call) {
    api->push_number(call, (double)current_thread());
}

static void export_fail(LuvCall* call) {
    api->fail(call, "this always fails");
}

static void export_describe(LuvCall* call) {
    int32_t count = api->arg_count(call);
    for (int32_t index = 0; index < count; index++) {
        api->push_number(call, api->arg_kind(call, index));
    }
}

static void export_echo(LuvCall* call) {
    int32_t count = api->arg_count(call);
    for (int32_t index = 0; index < count; index++) {
        api->push_argument(call, index);
    }
}

static void export_check_types(LuvCall* call) {
    api->check_number(call, 0);
    api->check_string(call, 1, NULL);
    api->push_boolean(call, 1);
}

static void export_slow_add(LuvCall* call) {
    double left = api->check_number(call, 0);
    double right = api->check_number(call, 1);
    pause_ms(100);
    api->push_number(call, left + right);
}

static void export_class_of(LuvCall* call) {
    const LuvClass* found = api->arg_class(call, 0);
    const LuvClass* named = api->find_class("Vec3");
    api->push_string(call, found ? api->class_name(found) : "none");
    api->push_boolean(call, named && strcmp(api->class_name(named), "Vec3") == 0 && !api->find_class("Missing"));
}

#ifdef _WIN32
static DWORD WINAPI run_ticker(LPVOID argument) {
#else
static void* run_ticker(void* argument) {
#endif
    Ticker* ticker = (Ticker*)argument;
    for (int32_t index = 1; index <= ticker->count; index++) {
        LuvCall* event = api->begin_event(ticker->handler);
        api->push_number(event, index);
        api->push_string(event, "tick");
        api->send_event(event);
        pause_ms(5);
    }
    api->release(ticker->handler);
    free(ticker);
#ifdef _WIN32
    return 0;
#else
    return NULL;
#endif
}

static void export_listen(LuvCall* call) {
    Ticker* ticker = (Ticker*)malloc(sizeof(Ticker));
    ticker->handler = api->retain(call, 0);
    ticker->count = (int32_t)api->check_number(call, 1);
    if (!ticker->handler) {
        free(ticker);
        api->fail(call, "listen needs a function");
        return;
    }
#ifdef _WIN32
    HANDLE thread = CreateThread(NULL, 0, run_ticker, ticker, 0, NULL);
    CloseHandle(thread);
#else
    pthread_t thread;
    pthread_create(&thread, NULL, run_ticker, ticker);
    pthread_detach(thread);
#endif
}

static LuvRef* held_signal = NULL;
static LuvRef* held_handler = NULL;
static LuvTask* held_task = NULL;
static double heard_number = 0;
static int32_t tick_count = 0;
static double service_level = 1;

static void on_signal(LuvCall* call) {
    heard_number = api->opt_number(call, 0, 0);
}

static void export_make_signal(LuvCall* call) {
    if (held_signal) {
        api->release(held_signal);
        held_signal = NULL;
    }
    held_signal = api->new_signal(call, "NativeSignal");
    if (!held_signal) {
        api->fail(call, "the signal could not be made");
        return;
    }
    api->connect(call, held_signal, "native", on_signal, NULL, LUV_INLINE);
    api->push_ref(call, held_signal);
}

static void export_heard(LuvCall* call) {
    api->push_number(call, heard_number);
}

static void doubler(LuvCall* call) {
    double value = api->check_number(call, 0);
    double* step = (double*)api->call_data(call);
    api->push_number(call, value * 2 + (step ? *step : 0));
}

static double doubler_step = 0.5;

static void export_make_function(LuvCall* call) {
    LuvRef* made = api->new_function(call, "doubler", doubler, &doubler_step, LUV_INLINE);
    api->push_ref(call, made);
    api->release(made);
}

static void export_members(LuvCall* call) {
    LuvValue target = luv_nil();
    if (api->arg_value(call, 0, &target) != LUV_OK || !target.handle) {
        api->fail(call, "members needs an object");
        return;
    }
    LuvValue name = luv_nil();
    if (api->read_member(call, target.handle, "Name", &name) == LUV_OK) {
        api->push_bytes(call, name.data, name.length);
    } else {
        api->push_nil(call);
    }
    LuvValue renamed = luv_bytes("renamed", 7);
    api->push_number(call, api->write_member(call, target.handle, "Name", &renamed));
    api->release(target.handle);
}

static void export_call_member(LuvCall* call) {
    LuvValue target = luv_nil();
    if (api->arg_value(call, 0, &target) != LUV_OK || !target.handle) {
        api->fail(call, "call_member needs an object");
        return;
    }
    LuvValue id = luv_bytes("native", 6);
    LuvValue results[2];
    int32_t written = api->call_member(call, target.handle, "IsBound", &id, 1, results, 2);
    api->push_number(call, written);
    api->push_boolean(call, written > 0 && results[0].numbers[0] != 0);
    api->release(target.handle);
}

static void export_samples(LuvCall* call) {
    double asked = api->check_number(call, 0);
    if (asked < 0) {
        asked = 0;
    }
    uint64_t count = (uint64_t)asked;
    uint8_t* room = (uint8_t*)api->push_buffer(call, count);
    if (!room) {
        return;
    }
    for (uint64_t index = 0; index < count; index++) {
        room[index] = (uint8_t)(index * 3 + 1);
    }
}

static void export_globals(LuvCall* call) {
    LuvValue text = luv_bytes("from the plugin", 15);
    api->push_number(call, api->set_global(call, "pluginGreeting", &text));
    LuvRef* found = api->get_global(call, "pluginGreeting");
    api->push_ref(call, found);
    api->release(found);
}

static void export_imported(LuvCall* call) {
    LuvRef* signals = api->get_import(call, "Signal");
    if (!signals) {
        api->fail(call, "Signal could not be imported");
        return;
    }
    LuvRef* made = api->construct(call, signals, "new", NULL, 0);
    api->push_ref(call, made);
    api->release(made);
    api->release(signals);
}

static void export_window_shape(LuvCall* call) {
    LuvValue window = luv_nil();
    if (api->arg_value(call, 0, &window) != LUV_OK || !window.handle) {
        api->fail(call, "window_shape needs a window");
        return;
    }
    LuvRef* renderable = api->get_api(call, window.handle, "Renderable");
    if (!renderable) {
        api->release(window.handle);
        return;
    }
    LuvValue arguments[1];
    arguments[0] = luv_bytes("RenderableShape", 15);
    LuvRef* made = api->construct(call, renderable, "new", arguments, 1);
    if (made) {
        LuvValue size = luv_udim(64, 48, 0);
        api->write_member(call, made, "Size", &size);
        api->push_ref(call, made);
        api->release(made);
    } else {
        api->push_nil(call);
    }
    api->release(renderable);
    api->release(window.handle);
}

static void on_tick(LuvCall* call) {
    (void)call;
    tick_count++;
    if (!held_handler) {
        return;
    }
    LuvCall* event = api->begin_event(held_handler);
    api->push_number(event, tick_count);
    api->send_event(event);
}

static void export_start_ticks(LuvCall* call) {
    if (held_handler) {
        api->release(held_handler);
        held_handler = NULL;
    }
    held_handler = api->retain(call, 0);
    tick_count = 0;
    held_task = api->schedule(call, "fixture ticks", on_tick, NULL, 0.002, LUV_INLINE);
    api->push_boolean(call, held_task != NULL);
}

static void export_stop_ticks(LuvCall* call) {
    if (held_task) {
        api->cancel(held_task);
        held_task = NULL;
    }
    if (held_handler) {
        api->release(held_handler);
        held_handler = NULL;
    }
    api->push_number(call, tick_count);
}

static void export_post_name(LuvCall* call) {
    LuvValue target = luv_nil();
    if (api->arg_value(call, 0, &target) != LUV_OK || !target.handle) {
        api->fail(call, "post_name needs an object");
        return;
    }
    LuvValue name = luv_bytes("posted", 6);
    api->push_number(call, api->post_write(target.handle, "Name", &name));
    LuvValue nothing = luv_nil();
    api->push_number(call, api->post_call(target.handle, "Fire", &nothing, 1));
    api->push_boolean(call, api->on_game_thread(call) == 0);
    api->release(target.handle);
}

static void export_release_signal(LuvCall* call) {
    (void)call;
    if (held_signal) {
        api->release(held_signal);
        held_signal = NULL;
    }
}

static void service_version(LuvCall* call) {
    api->push_number(call, LUV_API_VERSION);
}

static void service_greet(LuvCall* call) {
    static char text[128];
    const char* who = api->opt_string(call, 0, "world", NULL);
    snprintf(text, sizeof(text), "hello %s", who);
    api->push_string(call, text);
}

static void service_level_get(LuvCall* call) {
    api->push_number(call, service_level);
}

static void service_level_set(LuvCall* call) {
    service_level = api->check_number(call, 0);
}

static const LuvMethod service_functions[] = {
    {"Version", service_version, LUV_INLINE},
    {"Greet", service_greet, LUV_INLINE},
    {0},
};

static const LuvProperty service_properties[] = {
    {"Level", service_level_get, service_level_set},
    {0},
};

static void export_sideload(LuvCall* call) {
    uint64_t length = 0;
    const char* name = api->opt_string(call, 0, "plugin.png", NULL);
    const char* data = api->check_string(call, 1, &length);
    api->push_asset(call, name, data, length);
}

static const LuvMethod exported[] = {
    {"version", export_version, LUV_INLINE},
    {"inline_thread", export_inline_thread, LUV_INLINE},
    {"fail_on_purpose", export_fail, LUV_INLINE},
    {"describe", export_describe, LUV_INLINE},
    {"echo", export_echo, LUV_INLINE},
    {"check_types", export_check_types, LUV_INLINE},
    {"slow_add", export_slow_add, LUV_WORKER},
    {"class_of", export_class_of, LUV_INLINE},
    {"listen", export_listen, LUV_WORKER},
    {"make_signal", export_make_signal, LUV_INLINE},
    {"heard", export_heard, LUV_INLINE},
    {"release_signal", export_release_signal, LUV_INLINE},
    {"make_function", export_make_function, LUV_INLINE},
    {"members", export_members, LUV_INLINE},
    {"call_member", export_call_member, LUV_INLINE},
    {"samples", export_samples, LUV_INLINE},
    {"globals", export_globals, LUV_INLINE},
    {"imported", export_imported, LUV_INLINE},
    {"window_shape", export_window_shape, LUV_INLINE},
    {"start_ticks", export_start_ticks, LUV_INLINE},
    {"stop_ticks", export_stop_ticks, LUV_INLINE},
    {"post_name", export_post_name, LUV_WORKER},
    {"sideload", export_sideload, LUV_INLINE},
    {0},
};

LUV_EXPORT int32_t luv_register(const LuvApi* given, LuvRegistry* registry) {
    api = given;
    if (api->version < LUV_API_VERSION) {
        return LUV_INVALID;
    }
    LuvClassInfo vec3 = {"Vec3", sizeof(Vec3), NULL, vec3_methods, vec3_properties, vec3_statics, vec3_static_properties};
    LuvClassInfo counter = {"Counter", sizeof(Counter), counter_destroy, counter_methods, counter_properties, counter_statics, NULL};
    vec3_class = api->define_class(registry, &vec3);
    counter_class = api->define_class(registry, &counter);
    LuvServiceInfo fixture = {"Fixture", service_functions, service_properties};
    api->define_service(registry, &fixture);
    for (const LuvMethod* function = exported; function->name; function++) {
        api->define_function(registry, function);
    }
    return vec3_class && counter_class ? LUV_OK : LUV_INVALID;
}
