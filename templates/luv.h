#ifndef LUV_H
#define LUV_H

#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

#if defined(_WIN32)
#define LUV_EXPORT __declspec(dllexport)
#else
#define LUV_EXPORT __attribute__((visibility("default")))
#endif

#define LUV_RENDER_VERSION 1

#define LUV_OK 0
#define LUV_UNKNOWN_NAME -1
#define LUV_OUT_OF_RANGE -2
#define LUV_WRONG_KIND -3
#define LUV_INVALID -4

typedef struct LuvVulkan {
    void* instance;
    void* physical_device;
    void* device;
    void* queue;
    uint32_t queue_family;
    uint32_t queue_index;
    void* (*get_instance_proc_addr)(void* instance, const char* name);
} LuvVulkan;

typedef struct LuvRenderContext LuvRenderContext;

struct LuvRenderContext {
    uint32_t version;
    uint32_t struct_size;
    void* user_data;
    uint64_t renderable;
    double time;
    double delta;
    uint32_t frame;
    float width;
    float height;
    float scale;
    float position[2];
    float size[2];
    float anchor[2];
    float rotation;
    int32_t (*write_data)(LuvRenderContext* context, const char* name, uint64_t offset, const void* data, uint64_t length);
    int32_t (*write_texture)(LuvRenderContext* context, const char* name, uint32_t width, uint32_t height, const void* rgba);
    void (*set_draw_counts)(LuvRenderContext* context, uint32_t vertex_count, uint32_t instance_count);
    const LuvVulkan* vulkan;
    void* engine;
};

typedef void (*LuvRenderHook)(LuvRenderContext* context);

#define LUV_API_VERSION 1

#define LUV_WORKER 0
#define LUV_INLINE 1
#define LUV_PARALLEL 2

#define LUV_KIND_NONE -1
#define LUV_KIND_NIL 0
#define LUV_KIND_BOOLEAN 1
#define LUV_KIND_NUMBER 2
#define LUV_KIND_STRING 3
#define LUV_KIND_UDIM 4
#define LUV_KIND_COLOR 5
#define LUV_KIND_OBJECT 6
#define LUV_KIND_POINTER 7
#define LUV_KIND_VALUE 8

typedef struct LuvCall LuvCall;
typedef struct LuvClass LuvClass;
typedef struct LuvRegistry LuvRegistry;
typedef struct LuvRef LuvRef;

typedef void (*LuvFunction)(LuvCall* call);

typedef struct LuvMethod {
    const char* name;
    LuvFunction function;
    uint32_t flags;
} LuvMethod;

typedef struct LuvProperty {
    const char* name;
    LuvFunction get;
    LuvFunction set;
} LuvProperty;

typedef struct LuvClassInfo {
    const char* name;
    uint64_t size;
    void (*destroy)(void* data);
    const LuvMethod* methods;
    const LuvProperty* properties;
    const LuvMethod* statics;
    const LuvProperty* static_properties;
} LuvClassInfo;

typedef struct LuvApi {
    uint32_t version;
    uint32_t struct_size;
    const LuvClass* (*define_class)(LuvRegistry* registry, const LuvClassInfo* info);
    int32_t (*define_function)(LuvRegistry* registry, const LuvMethod* function);
    const LuvClass* (*find_class)(const char* name);
    const char* (*class_name)(const LuvClass* cls);
    int32_t (*arg_count)(LuvCall* call);
    int32_t (*arg_kind)(LuvCall* call, int32_t index);
    const LuvClass* (*arg_class)(LuvCall* call, int32_t index);
    int32_t (*check_boolean)(LuvCall* call, int32_t index);
    int32_t (*opt_boolean)(LuvCall* call, int32_t index, int32_t fallback);
    double (*check_number)(LuvCall* call, int32_t index);
    double (*opt_number)(LuvCall* call, int32_t index, double fallback);
    const char* (*check_string)(LuvCall* call, int32_t index, uint64_t* length);
    const char* (*opt_string)(LuvCall* call, int32_t index, const char* fallback, uint64_t* length);
    int32_t (*check_udim)(LuvCall* call, int32_t index, double* xyz);
    int32_t (*check_color)(LuvCall* call, int32_t index, double* rgba);
    void* (*check_object)(LuvCall* call, int32_t index, const LuvClass* cls);
    void* (*to_object)(LuvCall* call, int32_t index, const LuvClass* cls);
    void* (*check_pointer)(LuvCall* call, int32_t index);
    void* (*self_data)(LuvCall* call);
    void (*push_nil)(LuvCall* call);
    void (*push_boolean)(LuvCall* call, int32_t value);
    void (*push_number)(LuvCall* call, double value);
    void (*push_string)(LuvCall* call, const char* text);
    void (*push_bytes)(LuvCall* call, const void* data, uint64_t length);
    void (*push_udim)(LuvCall* call, double x, double y, double z);
    void (*push_color)(LuvCall* call, double r, double g, double b, double a);
    void* (*push_object)(LuvCall* call, const LuvClass* cls);
    void (*push_argument)(LuvCall* call, int32_t index);
    void (*push_self)(LuvCall* call);
    void (*push_pointer)(LuvCall* call, void* address);
    void (*push_ref)(LuvCall* call, LuvRef* ref);
    void (*fail)(LuvCall* call, const char* message);
    LuvRef* (*retain)(LuvCall* call, int32_t index);
    void (*release)(LuvRef* ref);
    LuvCall* (*begin_event)(LuvRef* function);
    int32_t (*send_event)(LuvCall* event);
    void (*print)(const char* message);
    void (*warn)(const char* message);
} LuvApi;

typedef int32_t (*LuvRegister)(const LuvApi* api, LuvRegistry* registry);

#ifdef __cplusplus
}
#endif

#endif
