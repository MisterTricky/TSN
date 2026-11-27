/*
 * tsn.h — C ABI for the tsn language runtime
 *
 * Draft v0.1, revision 3
 *   rev 2: per-request recovery, typed domain errors, scope API,
 *          host-import enforcement + escape hatch.
 *   rev 3: embedded error manifest, graceful cancellation, explicit
 *          by-value exception for the fixed word-pair view types.
 *
 * "tsn" is a placeholder prefix. Rename globally once the language has a name.
 *
 * Targets: linux/amd64 only (v0). SysV AMD64 psABI. No C++ ABI. No varargs.
 * Everything here is C99 + <stdint.h> + <stddef.h>.
 */

#ifndef TSN_H
#define TSN_H

#include <stdint.h>
#include <stddef.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Version                                                            */
/* ------------------------------------------------------------------ */

#define TSN_ABI_MAJOR 1u
#define TSN_ABI_MINOR 0u

/* Returns (MAJOR << 16) | MINOR. Adapters MUST check this at init. */
TSN_EXPORT uint32_t tsn_abi_version(void);

/* ------------------------------------------------------------------ */
/* Export macro                                                       */
/* ------------------------------------------------------------------ */

#if defined(_WIN32)
#  define TSN_EXPORT __declspec(dllexport)
#else
#  define TSN_EXPORT __attribute__((visibility("default")))
#endif

/* ------------------------------------------------------------------ */
/* Status codes                                                       */
/*                                                                    */
/* `code` is the TRANSPORT-level result. Domain-level identity lives   */
/* in tsn_error.domain_id (see below).                                 */
/* ------------------------------------------------------------------ */

typedef int32_t tsn_status;

enum {
    TSN_OK        = 0,
    TSN_EINVAL    = 1,  /* bad argument / malformed input (ABI-level) */
    TSN_ENOMEM    = 2,  /* allocation failed                          */
    TSN_EIO       = 3,  /* OS / IO failure                            */
    TSN_ECANCELED = 4,  /* cancelled via scope/runtime cancellation   */
    TSN_EPANIC    = 5,  /* uncaught exception / trap recovered at scope */
    TSN_EHOST     = 6,  /* host import unavailable or failed          */
    TSN_EVERSION  = 7,  /* ABI version mismatch                       */
    TSN_ETIMEOUT  = 8,
    TSN_EUSER     = 9   /* user-thrown domain error; read domain_id   */
};

/* ------------------------------------------------------------------ */
/* Borrowed views                                                     */
/*                                                                    */
/* Valid ONLY for the duration of the call that produced them,        */
/* unless the producer is an owned tsn_obj (then valid while retained).*/
/* UTF-8, NOT NUL-terminated, MAY contain interior NUL bytes.         */
/*                                                                    */
/* These four types are the ONLY compound values that may be passed    */
/* or returned BY VALUE. They are 1-2 word pairs classified            */
/* identically (INTEGER,INTEGER) by every psABI-conforming compiler.   */
/* Everything else crosses by pointer. See ABI §5.                     */
/* ------------------------------------------------------------------ */

typedef struct { const uint8_t* ptr; size_t len; } tsn_str;
typedef struct { const uint8_t* ptr; size_t len; } tsn_bytes;
typedef struct { uint8_t*       ptr; size_t len; } tsn_mut_bytes;

/* ------------------------------------------------------------------ */
/* Owned objects                                                      */
/*                                                                    */
/* Refcounted, thread-affine (see spec §11). Opaque to callers.        */
/* Immutable byte buffer + 16-byte type tag.                          */
/* ------------------------------------------------------------------ */

typedef struct tsn_obj tsn_obj;

typedef struct { uint8_t bytes[16]; } tsn_type_tag; /* e.g. "application/json" */

TSN_EXPORT tsn_obj*     tsn_obj_new(const uint8_t* data, size_t len, tsn_type_tag tag);
TSN_EXPORT tsn_obj*     tsn_obj_retain(tsn_obj* o);
TSN_EXPORT void         tsn_obj_release(tsn_obj* o);
TSN_EXPORT tsn_str      tsn_obj_data(const tsn_obj* o);   /* borrowed view */
TSN_EXPORT tsn_type_tag tsn_obj_tag(const tsn_obj* o);

/* ------------------------------------------------------------------ */
/* Typed domain errors                                                */
/*                                                                    */
/* A language-level `throw` produces:                                  */
/*   code      = TSN_EUSER                                            */
/*   domain_id = stable id of the concrete error type                 */
/*   payload   = CBOR-encoded structured error data (owned)           */
/*                                                                    */
/* domain_id is FNV-1a 64 over the UTF-8 string "module:TypeName".     */
/* 0 is reserved for "untyped". The compiler records every id in the  */
/* error manifest (embedded + tsn.errors.json).                       */
/*                                                                    */
/* Fast path is allocation-free: NULL payload + domain_id 0 mean      */
/* "no detail". Payload is allocated only on the error path.          */
/* ------------------------------------------------------------------ */

typedef struct {
    uint32_t  struct_size;  /* = sizeof(tsn_error) */
    tsn_status code;
    uint64_t  domain_id;    /* 0 = untyped */
    tsn_obj*  payload;      /* owned, nullable; CBOR */
} tsn_error;

TSN_EXPORT void     tsn_error_dispose(tsn_error* e);

/* Compute the same id the compiler emits, for C-side matching. */
TSN_EXPORT uint64_t tsn_domain_id(tsn_str qualified_name);

/* ------------------------------------------------------------------ */
/* Host table (npm leaf / embedding bridge)                           */
/*                                                                    */
/* NULL in pure-standalone builds with no host imports.               */
/* ------------------------------------------------------------------ */

typedef struct tsn_host {
    uint32_t struct_size;
    uint32_t abi_major;
    uint32_t abi_minor;
    uint64_t features;      /* TSN_FEATURE_* bitmask */

    /* Resolve a declared host import. Returns NULL if unavailable.
     * `sig` uses the grammar in spec §10. Result is cached by the
     * generated module glue; called once at module init. */
    void* (*resolve)(void* host_ctx,
                     const uint8_t* name, size_t name_len,
                     const uint8_t* sig,  size_t sig_len);

    /* Host allocator, for buffers that must outlive a single call. */
    void* (*alloc)(void* host_ctx, size_t n);
    void  (*free) (void* host_ctx, void* p);

    void* host_ctx;
} tsn_host;

enum {
    TSN_FEATURE_NAPI_VALUES = 1ull << 0, /* adapter may pass opaque napi_value handles */
    TSN_FEATURE_ZERO_COPY   = 1ull << 1, /* adapter may hand out views into host buffers */
    TSN_FEATURE_THREADS     = 1ull << 2  /* runtime may spawn worker threads */
};

/* ------------------------------------------------------------------ */
/* Runtime                                                            */
/*                                                                    */
/* Process-global, initialized once. Exports do not take a runtime    */
/* handle. Multi-instance is explicitly unsupported in v1.            */
/* ------------------------------------------------------------------ */

typedef struct {
    uint32_t        struct_size;   /* = sizeof(tsn_runtime_config) */
    const tsn_host* host;          /* NULL => pure standalone      */
    uint32_t        thread_count;  /* 0 => nproc                   */
    size_t          arena_size;    /* 0 => default                 */
    const char*     log_level;     /* NULL => "warn"               */
    uint32_t        host_call_warn_threshold; /* dev guard; 0 => off */
} tsn_runtime_config;

TSN_EXPORT tsn_status tsn_runtime_init(const tsn_runtime_config* cfg);
TSN_EXPORT void       tsn_runtime_shutdown(void);

/* Graceful shutdown: cancels every live scope and task, waits up to
 * `grace_ms` for them to unwind, then forces teardown. Idempotent.
 * In-flight tsn_scope_run calls return TSN_ECANCELED. */
TSN_EXPORT tsn_status tsn_runtime_cancel_all(uint32_t grace_ms);

/* Embedded error manifest: JSON, UTF-8, borrowed, valid for process
 * life. Source of truth emitted by the compiler; adapters use it to
 * reconstruct typed errors. A build-time copy is also written to
 * tsn.errors.json for tooling. */
TSN_EXPORT tsn_str tsn_manifest_json(void);

/* ------------------------------------------------------------------ */
/* Request scopes                                                     */
/*                                                                    */
/* A scope is simultaneously:                                         */
/*   - an arena  (all request-lifetime allocations)                   */
/*   - a cleanup stack (LIFO finalizers)                              */
/*   - a recovery point (catches uncaught exceptions and traps)       */
/*   - a cancellation node (child of the current token)               */
/*                                                                    */
/* Scopes nest. A recovered panic or cancellation tears the scope     */
/* down completely (finalizers + arena) and leaves the runtime        */
/* healthy.                                                           */
/* ------------------------------------------------------------------ */

typedef tsn_status (*tsn_handler_fn)(void* ctx, tsn_obj** out);

/* Runs `fn` inside a fresh scope on the current thread.
 *
 * On success:    returns TSN_OK,        *out = handler result (owned)
 * On exception:  returns TSN_EPANIC,    err describes the thrown error
 * On cancel:     returns TSN_ECANCELED, err carries an AbortError
 * On failure:    returns the handler's status
 *
 * `*out` MUST be heap-allocated (tsn_obj), NOT arena memory: the arena
 * is destroyed before this call returns. The runtime enforces this.
 */
TSN_EXPORT tsn_status tsn_scope_run(tsn_handler_fn fn, void* ctx,
                                    tsn_obj** out, tsn_error* err);

/* Allocate in the current scope's arena. Panics (TSN_ENOMEM) if the
 * scope is exhausted or there is no current scope. */
TSN_EXPORT void* tsn_scope_alloc(size_t n, size_t align);

/* Register a finalizer, run LIFO on scope teardown (normal, panic, or
 * cancellation). Use for resources that are not arena-allocated: fds,
 * pooled handles, refcounted objects held across a yield. */
typedef void (*tsn_finalizer_fn)(void* ctx);
TSN_EXPORT void  tsn_scope_on_cleanup(tsn_finalizer_fn fn, void* ctx);

/* Request cancellation of the current scope (and all descendants).
 * Idempotent. The scope unwinds at the next yield point. */
TSN_EXPORT void  tsn_scope_cancel(void);

/* 1 if the current scope has been cancelled. Cheap; call it in long
 * computations that do not yield. */
TSN_EXPORT int32_t tsn_scope_cancelled(void);

/* ------------------------------------------------------------------ */
/* Tasks (async crossing)                                             */
/*                                                                    */
/* A task is a unit of async work. It is NOT a JS Promise and NOT a   */
/* future; it is a completion handle. Adapters bridge it to the       */
/* host's async primitive.                                            */
/*                                                                    */
/* Every task body runs in its own request scope, as a child of the   */
/* spawning scope. Cancelling the parent cancels the task.            */
/* ------------------------------------------------------------------ */

typedef struct tsn_task tsn_task;

/* Invoked on the runtime's completion thread, NOT the host loop.
 * Adapters MUST hop back to the host loop (e.g. napi_threadsafe_function). */
typedef void (*tsn_task_done_fn)(tsn_task* t, void* user);

TSN_EXPORT tsn_task*  tsn_task_spawn(void* ctx, void (*fn)(void*));
TSN_EXPORT int32_t    tsn_task_poll(tsn_task* t);      /* 1 = done, 0 = pending */

/* Blocks the calling thread. MUST NOT be called on the host event loop. */
TSN_EXPORT void       tsn_task_wait(tsn_task* t);

TSN_EXPORT void       tsn_task_on_done(tsn_task* t, tsn_task_done_fn cb, void* user);

/* Takes ownership of the result. Returns TSN_EINVAL if not yet done. */
TSN_EXPORT tsn_status tsn_task_take_result(tsn_task* t, tsn_obj** out, tsn_error* err);

TSN_EXPORT tsn_status tsn_task_cancel(tsn_task* t);    /* cooperative */
TSN_EXPORT void       tsn_task_release(tsn_task* t);

/* ------------------------------------------------------------------ */
/* Panic observation                                                  */
/*                                                                    */
/* Uncaught exceptions and runtime traps are RECOVERED at the nearest */
/* scope; they never cross the C ABI. This hook observes them for     */
/* telemetry and MUST NOT return, throw, or longjmp.                  */
/*                                                                    */
/* Non-recoverable conditions (OOM, internal corruption) still abort. */
/* ------------------------------------------------------------------ */

typedef void (*tsn_panic_observer_fn)(tsn_str msg, uint64_t domain_id, void* user);
TSN_EXPORT void tsn_set_panic_observer(tsn_panic_observer_fn fn, void* user);

/* ------------------------------------------------------------------ */
/* Arenas (low-level; scopes use these internally)                    */
/*                                                                    */
/* Prefer scopes. Exposed for embedders that need an arena without a  */
/* recovery point.                                                    */
/* ------------------------------------------------------------------ */

typedef struct tsn_arena tsn_arena;

TSN_EXPORT tsn_arena* tsn_arena_create(size_t cap);
TSN_EXPORT void*      tsn_arena_alloc(tsn_arena* a, size_t n, size_t align);
TSN_EXPORT void       tsn_arena_reset(tsn_arena* a);
TSN_EXPORT void       tsn_arena_destroy(tsn_arena* a);

/* ------------------------------------------------------------------ */
/* Generated module entry points                                      */
/*                                                                    */
/* For a module `foo`, the compiler emits:                            */
/*   tsn_status tsn_module_foo_init(void);                            */
/*   void       tsn_module_foo_deinit(void);                          */
/*   <generated exports>                                              */
/*                                                                    */
/* `init` resolves all declared host imports and returns TSN_EHOST    */
/* if any is missing and the module did not declare it optional.      */
/*                                                                    */
/* The compiler also emits tsn.errors.json:                           */
/*   { "manifest_version": 1, "abi_major": 1, "abi_minor": 0,         */
/*     "errors": [ { "id": <u64>, "name": "app:NotFoundError",        */
/*                   "fields": [ {"name":"id","type":"str"} ] } ] }   */
/* ------------------------------------------------------------------ */

#ifdef __cplusplus
}
#endif
#endif /* TSN_H */
