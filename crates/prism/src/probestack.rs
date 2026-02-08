// Workaround for musl static builds on recent Rust toolchains.
//
// When building for `*-unknown-linux-musl`, LLVM may emit a call to the runtime
// symbol `__rust_probestack` (similar to `__chkstk`) for stack probing.
//
// As of Rust 1.93 (LLVM 21), the musl target's `libcompiler_builtins` in the
// rust-musl builder images provides a probestack implementation, but it is not
// exported under the unmangled C ABI name `__rust_probestack`, which can cause
// link failures like:
//
//   undefined reference to `__rust_probestack`
//
// We provide a compatible implementation here, exported as a *weak* symbol so
// that if/when the toolchain starts providing a proper definition again, the
// toolchain's definition will win without causing duplicate-symbol errors.

#![allow(unsafe_code)]

#[cfg(all(target_os = "linux", target_env = "musl", target_arch = "x86_64"))]
core::arch::global_asm!(
    r#"
    .intel_syntax noprefix
    .text
    .globl __rust_probestack
    .weak __rust_probestack
    .type __rust_probestack,@function
__rust_probestack:
    push rbp
    mov rbp, rsp

    // RAX = size
    mov r11, rax
    cmp r11, 0x1000
    jbe 2f

1:
    sub rsp, 0x1000
    test qword ptr [rsp+8], rsp
    sub r11, 0x1000
    cmp r11, 0x1000
    ja 1b

2:
    sub rsp, r11
    test qword ptr [rsp+8], rsp
    add rsp, rax
    leave
    ret
    .size __rust_probestack, .-__rust_probestack
"#,
);

#[cfg(all(target_os = "linux", target_env = "musl", target_arch = "aarch64"))]
core::arch::global_asm!(
    r#"
    .text
    .globl __rust_probestack
    .weak __rust_probestack
    .type __rust_probestack,%function
__rust_probestack:
    // x0 = size
    cbz x0, 3f

    mov x9, x0
    cmp x9, #4096
    b.ls 2f

1:
    sub sp, sp, #4096
    // Touch the page.
    strb wzr, [sp]

    sub x9, x9, #4096
    cmp x9, #4096
    b.hi 1b

2:
    sub sp, sp, x9
    strb wzr, [sp]

    // Restore original SP.
    add sp, sp, x0

3:
    ret
    .size __rust_probestack, .-__rust_probestack
"#,
);
