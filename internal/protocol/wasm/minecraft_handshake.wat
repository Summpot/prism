;; Prism builtin WASM routing parser: minecraft_handshake
;;
;; Implements DESIGN.md "WASM Routing Parser ABI (v1)".
;; Exports:
;;   - memory
;;   - prism_parse(input_len:i32) -> i64
;;
;; Parsing behavior matches the Go builtin `MinecraftHostParser`:
;;   - return 0 (need more) when the length VarInt or the full packet isn't fully present
;;   - return 1 (no match) when framing/fields are invalid or not a handshake
;;   - otherwise return packed (ptr,len) of the server_address bytes

(module
  (memory (export "memory") 1)

  (func $pack (param $ptr i32) (param $len i32) (result i64)
    (i64.or
      (i64.extend_i32_u (local.get $ptr))
      (i64.shl (i64.extend_i32_u (local.get $len)) (i64.const 32))
    )
  )

  (func (export "prism_parse") (param $n i32) (result i64)
    (local $off i32)
    (local $val i32)
    (local $shift i32)
    (local $b i32)

    (local $packetLen i32)
    (local $nLen i32)
    (local $limit i32)

    (local $packetID i32)
    (local $addrLen i32)

    (block $ret (result i64)
      ;; Need at least 1 byte to make progress.
      (if (i32.eqz (local.get $n)) (then (br $ret (i64.const 0))))

      ;; ---- decode packet_length VarInt at offset 0 (EOF => need more) ----
      (local.set $off (i32.const 0))
      (local.set $val (i32.const 0))
      (local.set $shift (i32.const 0))
      (block $done_len
        (loop $loop_len
          (if (i32.ge_u (local.get $off) (local.get $n))
            (then (br $ret (i64.const 0)))
          )
          (local.set $b (i32.load8_u (local.get $off)))
          (local.set $off (i32.add (local.get $off) (i32.const 1)))
          (local.set $val
            (i32.or
              (local.get $val)
              (i32.shl
                (i32.and (local.get $b) (i32.const 127))
                (local.get $shift)
              )
            )
          )
          (if (i32.eqz (i32.and (local.get $b) (i32.const 128)))
            (then (br $done_len))
          )
          (local.set $shift (i32.add (local.get $shift) (i32.const 7)))
          ;; VarInt is at most 5 bytes (shift<=28). Anything beyond is invalid.
          (if (i32.gt_s (local.get $shift) (i32.const 28))
            (then (br $ret (i64.const 1)))
          )
          (br $loop_len)
        )
      )

      (local.set $packetLen (local.get $val))
      (local.set $nLen (local.get $off))

      (if (i32.le_s (local.get $packetLen) (i32.const 0)) (then (br $ret (i64.const 1))))
      ;; Safety cap (matches Go default 256KiB).
      (if (i32.gt_u (local.get $packetLen) (i32.const 262144)) (then (br $ret (i64.const 1))))

      ;; Require full packet bytes.
      (if (i32.gt_u (i32.add (local.get $nLen) (local.get $packetLen)) (local.get $n))
        (then (br $ret (i64.const 0)))
      )

      (local.set $off (local.get $nLen))
      (local.set $limit (i32.add (local.get $nLen) (local.get $packetLen)))

      ;; ---- decode packet_id VarInt (EOF within packet => no match) ----
      (local.set $val (i32.const 0))
      (local.set $shift (i32.const 0))
      (block $done_pid
        (loop $loop_pid
          (if (i32.ge_u (local.get $off) (local.get $limit))
            (then (br $ret (i64.const 1)))
          )
          (local.set $b (i32.load8_u (local.get $off)))
          (local.set $off (i32.add (local.get $off) (i32.const 1)))
          (local.set $val
            (i32.or
              (local.get $val)
              (i32.shl
                (i32.and (local.get $b) (i32.const 127))
                (local.get $shift)
              )
            )
          )
          (if (i32.eqz (i32.and (local.get $b) (i32.const 128)))
            (then (br $done_pid))
          )
          (local.set $shift (i32.add (local.get $shift) (i32.const 7)))
          (if (i32.gt_s (local.get $shift) (i32.const 28))
            (then (br $ret (i64.const 1)))
          )
          (br $loop_pid)
        )
      )
      (local.set $packetID (local.get $val))
      (if (i32.ne (local.get $packetID) (i32.const 0)) (then (br $ret (i64.const 1))))

      ;; ---- decode protocol_version VarInt (skip) ----
      (local.set $val (i32.const 0))
      (local.set $shift (i32.const 0))
      (block $done_proto
        (loop $loop_proto
          (if (i32.ge_u (local.get $off) (local.get $limit))
            (then (br $ret (i64.const 1)))
          )
          (local.set $b (i32.load8_u (local.get $off)))
          (local.set $off (i32.add (local.get $off) (i32.const 1)))
          (local.set $val
            (i32.or
              (local.get $val)
              (i32.shl
                (i32.and (local.get $b) (i32.const 127))
                (local.get $shift)
              )
            )
          )
          (if (i32.eqz (i32.and (local.get $b) (i32.const 128)))
            (then (br $done_proto))
          )
          (local.set $shift (i32.add (local.get $shift) (i32.const 7)))
          (if (i32.gt_s (local.get $shift) (i32.const 28))
            (then (br $ret (i64.const 1)))
          )
          (br $loop_proto)
        )
      )

      ;; ---- decode server_address length VarInt ----
      (local.set $val (i32.const 0))
      (local.set $shift (i32.const 0))
      (block $done_addr
        (loop $loop_addr
          (if (i32.ge_u (local.get $off) (local.get $limit))
            (then (br $ret (i64.const 1)))
          )
          (local.set $b (i32.load8_u (local.get $off)))
          (local.set $off (i32.add (local.get $off) (i32.const 1)))
          (local.set $val
            (i32.or
              (local.get $val)
              (i32.shl
                (i32.and (local.get $b) (i32.const 127))
                (local.get $shift)
              )
            )
          )
          (if (i32.eqz (i32.and (local.get $b) (i32.const 128)))
            (then (br $done_addr))
          )
          (local.set $shift (i32.add (local.get $shift) (i32.const 7)))
          (if (i32.gt_s (local.get $shift) (i32.const 28))
            (then (br $ret (i64.const 1)))
          )
          (br $loop_addr)
        )
      )
      (local.set $addrLen (local.get $val))

      ;; Negative (signed) lengths are invalid.
      (if (i32.lt_s (local.get $addrLen) (i32.const 0)) (then (br $ret (i64.const 1))))
      ;; Safety cap / Minecraft max hostname length.
      (if (i32.gt_u (local.get $addrLen) (i32.const 255)) (then (br $ret (i64.const 1))))

      ;; Require full string bytes inside the packet.
      (if (i32.gt_u (i32.add (local.get $off) (local.get $addrLen)) (local.get $limit))
        (then (br $ret (i64.const 1)))
      )

      ;; Return pointer to the raw host bytes (Prism normalizes casing/space in Go).
      (br $ret (call $pack (local.get $off) (local.get $addrLen)))
    )
  )
)
