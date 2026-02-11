;; Prism middleware: minecraft_handshake (parse host only)
;;
;; - Parse phase: extracts the Minecraft handshake "server address" string as routing host.
;;   If the string contains a NUL byte (Bungee/Velocity extra data), only the prefix before NUL is returned.
;; - Rewrite phase: no-op.

(module
  (memory (export "memory") 4)

  (func $pack (param $ptr i32) (param $len i32) (result i64)
    (i64.or
      (i64.extend_i32_u (local.get $ptr))
      (i64.shl (i64.extend_i32_u (local.get $len)) (i64.const 32))
    )
  )

  ;; read_varint(ptr, end) -> i64 { value:u32 (low32), nbytes:u32 (high32) }
  ;; nbytes==0 means need more data.
  (func $read_varint (param $ptr i32) (param $end i32) (result i64)
    (local $i i32)
    (local $shift i32)
    (local $res i32)
    (local $b i32)

    (local.set $i (local.get $ptr))
    (local.set $shift (i32.const 0))
    (local.set $res (i32.const 0))

    (block $done
      (loop $loop
        (br_if $done (i32.ge_u (local.get $i) (local.get $end)))

        (local.set $b (i32.load8_u (local.get $i)))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))

        (local.set $res
          (i32.or
            (local.get $res)
            (i32.shl
              (i32.and (local.get $b) (i32.const 0x7f))
              (local.get $shift)
            )
          )
        )

        (br_if $done (i32.eq (i32.and (local.get $b) (i32.const 0x80)) (i32.const 0)))

        (local.set $shift (i32.add (local.get $shift) (i32.const 7)))
        ;; too long => treat as need more (caller will consider non-match)
        (br_if $done (i32.gt_s (local.get $shift) (i32.const 28)))

        (br $loop)
      )
    )

    ;; if we stopped because we hit end or varint too long without termination, signal need more
    (if (result i64)
      (i32.eq (i32.and (local.get $b) (i32.const 0x80)) (i32.const 0))
      (then
        (call $pack
          (local.get $res)
          (i32.sub (local.get $i) (local.get $ptr))
        )
      )
      (else
        (call $pack (i32.const 0) (i32.const 0))
      )
    )
  )

  (func $write_out_host (param $host_ptr i32) (param $host_len i32) (result i64)
    ;; out struct at 65536: { host_ptr, host_len, rw_ptr, rw_len }
    (i32.store (i32.const 65536) (local.get $host_ptr))
    (i32.store (i32.const 65540) (local.get $host_len))
    (i32.store (i32.const 65544) (i32.const 0))
    (i32.store (i32.const 65548) (i32.const 0))
    (call $pack (i32.const 65536) (i32.const 16))
  )

  (func (export "prism_mw_run") (param $n i32) (param $ctx i32) (result i64)
    (local $phase i32)
    (local $p i32)
    (local $end i32)

    (local $pkt_len i32)
    (local $len_n i32)

    (local $pid i32)
    (local $tmp i64)

    (local $addr_len i32)
    (local $addr_n i32)
    (local $addr_ptr i32)

    (local $i i32)
    (local $host_len i32)
    (local $b i32)

    ;; phase
    (local.set $phase (i32.load (i32.add (local.get $ctx) (i32.const 4))))
    (if (i32.ne (local.get $phase) (i32.const 0))
      (then (return (i64.const 1)))
    )

    ;; packet length
    (local.set $tmp (call $read_varint (i32.const 0) (local.get $n)))
    (local.set $pkt_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $len_n) (i32.const 0))
      (then (return (i64.const 0)))
    )

    (if (i32.lt_s (local.get $pkt_len) (i32.const 0))
      (then (return (i64.const 1)))
    )

    (local.set $p (local.get $len_n))
    (local.set $end (i32.add (local.get $len_n) (local.get $pkt_len)))
    (if (i32.gt_u (local.get $end) (local.get $n))
      (then (return (i64.const 0)))
    )

    ;; packet id
    (local.set $tmp (call $read_varint (local.get $p) (local.get $end)))
    (local.set $pid (i32.wrap_i64 (local.get $tmp)))
    (local.set $len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $len_n) (i32.const 0))
      (then (return (i64.const 0)))
    )
    (if (i32.ne (local.get $pid) (i32.const 0))
      (then (return (i64.const 1)))
    )
    (local.set $p (i32.add (local.get $p) (local.get $len_n)))

    ;; protocol version (skip)
    (local.set $tmp (call $read_varint (local.get $p) (local.get $end)))
    (local.set $len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $len_n) (i32.const 0))
      (then (return (i64.const 0)))
    )
    (local.set $p (i32.add (local.get $p) (local.get $len_n)))

    ;; address length
    (local.set $tmp (call $read_varint (local.get $p) (local.get $end)))
    (local.set $addr_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $addr_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $addr_n) (i32.const 0))
      (then (return (i64.const 0)))
    )
    (if (i32.lt_s (local.get $addr_len) (i32.const 0))
      (then (return (i64.const 1)))
    )

    (local.set $addr_ptr (i32.add (local.get $p) (local.get $addr_n)))
    (if (i32.gt_u (i32.add (i32.add (local.get $addr_ptr) (local.get $addr_len)) (i32.const 2)) (local.get $end))
      (then (return (i64.const 0)))
    )

    ;; scan for NUL within addr
    (local.set $i (i32.const 0))
    (local.set $host_len (local.get $addr_len))
    (block $scan_done
      (loop $scan
        (br_if $scan_done (i32.ge_u (local.get $i) (local.get $addr_len)))
        (local.set $b (i32.load8_u (i32.add (local.get $addr_ptr) (local.get $i))))
        (if (i32.eq (local.get $b) (i32.const 0))
          (then
            (local.set $host_len (local.get $i))
            (br $scan_done)
          )
        )
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $scan)
      )
    )

    (if (i32.eq (local.get $host_len) (i32.const 0))
      (then (return (i64.const 1)))
    )

    (return (call $write_out_host (local.get $addr_ptr) (local.get $host_len)))
  )
)
