# Binding Generation Workflow

Generating Foreign Function Interface (FFI) bindings to call libraries written in one language from another - wrapping C libraries for Rust, exposing Rust to Python, etc.

## Trigger

- Need to use existing C/C++ library from higher-level language
- Want to expose your library to other languages
- Performance-critical code needs native implementation
- Reusing battle-tested libraries instead of reimplementing

## Goal

- Working bindings that correctly call foreign code
- Type-safe interface in target language
- Proper memory management across boundary
- Idiomatic API in target language (not just raw FFI)

## Prerequisites

- Header files or documentation for source library
- Understanding of both languages' memory models
- Build system that can link both
- Test cases (ideally from original library)

## Why Binding Generation Is Hard

1. **Type system mismatch**: C's types don't map cleanly to Rust/Python/etc.
2. **Memory ownership**: Who allocates? Who frees? When?
3. **Error handling**: C returns codes, Rust wants Results, Python wants exceptions
4. **Strings**: Null-terminated? Length-prefixed? Encoding?
5. **Callbacks**: Function pointers across language boundaries
6. **ABI stability**: Struct layout, calling conventions

## Types of Bindings

| Direction | Example | Complexity |
|-----------|---------|------------|
| C → Rust | Wrap OpenSSL for Rust | Medium |
| C → Python | Wrap libcurl for Python | Medium (ctypes) to Low (existing tools) |
| Rust → Python | Expose Rust lib to Python | Low (PyO3) |
| Rust → C | Make Rust lib callable from C | Medium |
| C++ → Rust | Wrap Qt for Rust | High (C++ complexity) |
| Any → WASM | Compile to WebAssembly | Varies |

## Core Strategy: Analyze → Generate → Wrap → Test

```
┌─────────────────────────────────────────────────────────┐
│                      ANALYZE                             │
│  Understand the source API: types, functions, semantics │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                     GENERATE                             │
│  Create raw FFI declarations                            │
│  Often automated with bindgen/cbindgen                  │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       WRAP                               │
│  Build safe, idiomatic API on top of raw FFI           │
│  Handle memory, errors, types properly                  │
└─────────────────────┬───────────────────────────────────┘
                      ▼
┌─────────────────────────────────────────────────────────┐
│                       TEST                               │
│  Verify correctness, memory safety, edge cases         │
└─────────────────────────────────────────────────────────┘
```

## Phase 1: Analyze the Source API

### Read Headers/Documentation

```c
// Example: Simple C library header
// libfoo.h

typedef struct Foo {
    int value;
    char* name;  // Who owns this? When freed?
} Foo;

// Creates a new Foo (caller must free with foo_destroy)
Foo* foo_create(const char* name, int value);

// Destroys a Foo (frees internal memory too)
void foo_destroy(Foo* foo);

// Returns 0 on success, -1 on error (sets errno)
int foo_process(Foo* foo, const char* input, char** output);
```

### Document Key Questions

```markdown
## API Analysis: libfoo

### Memory Ownership
- `foo_create`: Returns owned pointer, caller must free
- `foo_destroy`: Frees the Foo AND its internal name string
- `foo_process`: `output` is allocated by function, caller frees

### Error Handling
- Returns -1 on error, sets errno
- No way to get error message (just errno)

### String Handling
- All strings are null-terminated C strings
- `name` in struct is owned copy (strdup'd internally)
- `input` is borrowed (not modified)
- `output` is newly allocated

### Thread Safety
- Not documented - assume NOT thread-safe
- Need mutex if called from multiple threads
```

### Identify Tricky Patterns

```c
// Callback pattern
typedef void (*foo_callback)(void* user_data, int event);
void foo_set_callback(Foo* foo, foo_callback cb, void* user_data);

// Opaque handle pattern
typedef struct FooContext FooContext;  // Incomplete type
FooContext* foo_context_new(void);

// Array output pattern
int foo_get_items(Foo* foo, Item** items, size_t* count);
// Who owns items array? Each item?

// Builder pattern
FooBuilder* foo_builder_new(void);
FooBuilder* foo_builder_set_name(FooBuilder* b, const char* name);
Foo* foo_builder_build(FooBuilder* b);  // Consumes builder?
```

## Phase 2: Generate Raw FFI

### Automated Generation (Preferred)

**Rust (bindgen)**:
```bash
# Generate Rust FFI from C headers
bindgen libfoo.h -o src/ffi.rs

# With configuration
bindgen libfoo.h \
    --allowlist-function "foo_.*" \
    --allowlist-type "Foo.*" \
    --no-layout-tests \
    -o src/ffi.rs
```

```rust
// Generated ffi.rs (simplified)
#[repr(C)]
pub struct Foo {
    pub value: ::std::os::raw::c_int,
    pub name: *mut ::std::os::raw::c_char,
}

extern "C" {
    pub fn foo_create(name: *const c_char, value: c_int) -> *mut Foo;
    pub fn foo_destroy(foo: *mut Foo);
    pub fn foo_process(
        foo: *mut Foo,
        input: *const c_char,
        output: *mut *mut c_char,
    ) -> c_int;
}
```

**Python (ctypes/cffi)**:
```python
# ctypes - manual but simple
from ctypes import *

libfoo = CDLL("libfoo.so")

class Foo(Structure):
    _fields_ = [
        ("value", c_int),
        ("name", c_char_p),
    ]

libfoo.foo_create.argtypes = [c_char_p, c_int]
libfoo.foo_create.restype = POINTER(Foo)

libfoo.foo_destroy.argtypes = [POINTER(Foo)]
libfoo.foo_destroy.restype = None
```

```python
# cffi - can parse headers directly
from cffi import FFI
ffi = FFI()

ffi.cdef("""
    typedef struct { int value; char* name; } Foo;
    Foo* foo_create(const char* name, int value);
    void foo_destroy(Foo* foo);
""")

lib = ffi.dlopen("libfoo.so")
```

**Rust → Python (PyO3)**:
```rust
use pyo3::prelude::*;

#[pyclass]
struct MyClass {
    value: i32,
}

#[pymethods]
impl MyClass {
    #[new]
    fn new(value: i32) -> Self {
        MyClass { value }
    }

    fn process(&self, input: &str) -> PyResult<String> {
        Ok(format!("Processed: {}", input))
    }
}

#[pymodule]
fn my_module(_py: Python, m: &PyModule) -> PyResult<()> {
    m.add_class::<MyClass>()?;
    Ok(())
}
```

### Manual FFI (When Needed)

```rust
// When bindgen can't handle it (C++, complex macros)
#[repr(C)]
pub struct Foo {
    value: c_int,
    name: *mut c_char,
}

#[link(name = "foo")]
extern "C" {
    fn foo_create(name: *const c_char, value: c_int) -> *mut Foo;
    fn foo_destroy(foo: *mut Foo);
}
```

## Phase 3: Build Safe Wrapper

### Memory Safety Wrapper (Rust)

```rust
// Raw FFI (unsafe)
mod ffi {
    // ... generated bindings
}

// Safe wrapper
pub struct Foo {
    ptr: *mut ffi::Foo,
}

impl Foo {
    pub fn new(name: &str, value: i32) -> Result<Self, Error> {
        let c_name = CString::new(name)?;
        let ptr = unsafe { ffi::foo_create(c_name.as_ptr(), value) };
        if ptr.is_null() {
            return Err(Error::CreateFailed);
        }
        Ok(Foo { ptr })
    }

    pub fn process(&mut self, input: &str) -> Result<String, Error> {
        let c_input = CString::new(input)?;
        let mut output: *mut c_char = std::ptr::null_mut();

        let result = unsafe {
            ffi::foo_process(self.ptr, c_input.as_ptr(), &mut output)
        };

        if result != 0 {
            return Err(Error::ProcessFailed(errno()));
        }

        // Take ownership of output string
        let output_str = unsafe {
            CStr::from_ptr(output).to_string_lossy().into_owned()
        };
        unsafe { libc::free(output as *mut c_void) };

        Ok(output_str)
    }
}

impl Drop for Foo {
    fn drop(&mut self) {
        unsafe { ffi::foo_destroy(self.ptr) };
    }
}

// Now Foo is Send + !Sync (unless library is thread-safe)
unsafe impl Send for Foo {}
```

### Error Handling Wrapper

```rust
// C returns -1, sets errno
// Rust wants Result<T, E>

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Operation failed: {0}")]
    OperationFailed(i32),

    #[error("Invalid string")]
    InvalidString(#[from] std::ffi::NulError),

    #[error("Null pointer returned")]
    NullPointer,
}

fn check_result(code: c_int) -> Result<(), Error> {
    if code == 0 {
        Ok(())
    } else {
        Err(Error::OperationFailed(errno()))
    }
}
```

### Callback Wrapper

```rust
// C callback: void (*callback)(void* user_data, int event)
// Rust: want closure

type RustCallback = Box<dyn FnMut(i32) + Send>;

extern "C" fn callback_trampoline(user_data: *mut c_void, event: c_int) {
    let callback = unsafe { &mut *(user_data as *mut RustCallback) };
    callback(event);
}

impl Foo {
    pub fn set_callback<F>(&mut self, callback: F)
    where
        F: FnMut(i32) + Send + 'static,
    {
        let boxed: RustCallback = Box::new(callback);
        let raw = Box::into_raw(Box::new(boxed));

        unsafe {
            ffi::foo_set_callback(
                self.ptr,
                Some(callback_trampoline),
                raw as *mut c_void,
            );
        }

        // Store raw pointer to free later
        self.callback = Some(raw);
    }
}
```

### Python Wrapper

```python
class Foo:
    """Safe Python wrapper for libfoo."""

    def __init__(self, name: str, value: int):
        self._ptr = lib.foo_create(name.encode(), value)
        if not self._ptr:
            raise RuntimeError("Failed to create Foo")

    def __del__(self):
        if hasattr(self, '_ptr') and self._ptr:
            lib.foo_destroy(self._ptr)

    def process(self, input: str) -> str:
        output = ffi.new("char**")
        result = lib.foo_process(self._ptr, input.encode(), output)
        if result != 0:
            raise RuntimeError(f"Process failed: {result}")
        try:
            return ffi.string(output[0]).decode()
        finally:
            lib.free(output[0])

    def __enter__(self):
        return self

    def __exit__(self, *args):
        if self._ptr:
            lib.foo_destroy(self._ptr)
            self._ptr = None
```

## Phase 4: Testing

### Port Original Tests

```rust
// If library has tests, port them
#[test]
fn test_create_and_destroy() {
    let foo = Foo::new("test", 42).unwrap();
    assert_eq!(foo.value(), 42);
    // Drop handles cleanup
}

#[test]
fn test_process() {
    let mut foo = Foo::new("test", 42).unwrap();
    let result = foo.process("input").unwrap();
    assert_eq!(result, "expected output");
}
```

### Memory Safety Tests

```rust
// Test that we don't leak memory
#[test]
fn test_no_memory_leak() {
    for _ in 0..1000 {
        let foo = Foo::new("test", 42).unwrap();
        drop(foo);
    }
    // Run under valgrind/miri
}

// Test use-after-free protection
#[test]
fn test_drop_invalidates() {
    let foo = Foo::new("test", 42).unwrap();
    drop(foo);
    // foo is gone, can't use it (compiler enforces)
}
```

### Miri for Undefined Behavior

```bash
# Rust: Run under Miri to detect UB
cargo +nightly miri test

# Common issues Miri catches:
# - Use after free
# - Invalid pointer dereference
# - Incorrect alignment
# - Data races
```

### Fuzzing

```rust
// Fuzz the bindings
#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if let Ok(s) = std::str::from_utf8(data) {
        if let Ok(mut foo) = Foo::new(s, 0) {
            let _ = foo.process(s);
        }
    }
});
```

## Common Patterns

### Opaque Handle Pattern

```c
// C: Incomplete type, only pointers allowed
typedef struct Context Context;
Context* context_new(void);
void context_free(Context* ctx);
```

```rust
// Rust: Use PhantomData for type safety
#[repr(C)]
pub struct Context {
    _private: [u8; 0],
    _marker: PhantomData<(*mut u8, PhantomPinned)>,
}

pub struct SafeContext {
    ptr: NonNull<Context>,
}
```

### String Conversion

```rust
// C string → Rust
fn c_str_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(|s| s.to_owned())
}

// Rust → C string (temporary)
fn with_c_str<F, R>(s: &str, f: F) -> Result<R, NulError>
where
    F: FnOnce(*const c_char) -> R,
{
    let c_str = CString::new(s)?;
    Ok(f(c_str.as_ptr()))
}
```

### Array Output Pattern

```c
// C: Returns array and count
int get_items(Item** items, size_t* count);
void free_items(Item* items, size_t count);
```

```rust
pub fn get_items() -> Result<Vec<Item>, Error> {
    let mut items: *mut ffi::Item = std::ptr::null_mut();
    let mut count: usize = 0;

    let result = unsafe { ffi::get_items(&mut items, &mut count) };
    check_result(result)?;

    let slice = unsafe { std::slice::from_raw_parts(items, count) };
    let vec: Vec<Item> = slice.iter().map(|i| Item::from_ffi(i)).collect();

    unsafe { ffi::free_items(items, count) };

    Ok(vec)
}
```

## Tools Reference

| Language Pair | Tool | Notes |
|--------------|------|-------|
| C → Rust | bindgen | Generates from headers |
| Rust → C | cbindgen | Generates C headers from Rust |
| Rust → Python | PyO3, rust-cpython | Full Python integration |
| Rust → Node | neon, napi-rs | N-API bindings |
| C → Python | ctypes, cffi, SWIG | Various approaches |
| C++ → Rust | cxx, autocxx | C++ is harder than C |
| Any → WASM | wasm-bindgen | WebAssembly bindings |

## LLM-Specific Techniques

### Header Analysis

```
Given this C header:
```c
typedef struct Buffer {
    void* data;
    size_t len;
    size_t cap;
} Buffer;

Buffer* buffer_new(size_t initial_cap);
int buffer_append(Buffer* buf, const void* data, size_t len);
void buffer_free(Buffer* buf);
```

Generate:
1. Rust FFI declarations
2. Safe wrapper with proper ownership
3. Test cases
```

### Memory Model Documentation

```
Analyze this API and document:
1. Who owns each allocation?
2. When is memory freed?
3. What are the lifetime relationships?
4. Thread safety guarantees?
```

## Failure Modes

| Failure | Detection | Recovery |
|---------|-----------|----------|
| Memory leak | Valgrind, ASAN | Add Drop impl, check ownership |
| Use after free | Miri, ASAN | Fix lifetime tracking |
| Type mismatch | Crash, corruption | Verify repr(C), check sizes |
| ABI mismatch | Crash | Check calling convention |

## Open Questions

### C++ Bindings

C++ is significantly harder than C:
- Name mangling
- Exceptions
- Templates
- RTTI
- Multiple inheritance

Tools like cxx help but have limitations. When is manual wrapping better?

### Async/Await Across FFI

How to handle async Rust calling blocking C?
- Spawn blocking task?
- Use async-compatible library version?

### Version Compatibility

Library updates may break ABI:
- Pin to exact version?
- Detect at runtime?
- Regenerate bindings per version?

## See Also

- [Code Synthesis](code-synthesis.md) - D×C verification applies
- [Cross-Language Migration](cross-language-migration.md) - Related concepts
