;; Prism middleware: minecraft_handshake (parse + rewrite)
;;
;; - Parse phase: extracts the Minecraft handshake "server address" string as routing host.
;;   If the string contains a NUL byte (Bungee/Velocity extra data), only the prefix before NUL is returned.
;; - Rewrite phase: rewrites the handshake host (and, if present, the port) to the selected upstream.
;;   - The server address string is rewritten to the upstream host only (no ":port").
;;   - If selected_upstream includes a numeric port, the u16 port field is rewritten as well.

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

  (func $write_u16be (param $p i32) (param $v i32)
    (i32.store8 (local.get $p) (i32.and (i32.shr_u (local.get $v) (i32.const 8)) (i32.const 0xff)))
    (i32.store8 (i32.add (local.get $p) (i32.const 1)) (i32.and (local.get $v) (i32.const 0xff)))
  )

  ;; Wasm text has no built-in memcpy; implement a simple loop.
  (func $memcpy_impl (param $dst i32) (param $src i32) (param $n i32)
    (local $i i32)
    (local $b i32)
    (local.set $i (i32.const 0))
    (block $done
      (loop $l
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $b (i32.load8_u (i32.add (local.get $src) (local.get $i))))
        (i32.store8 (i32.add (local.get $dst) (local.get $i)) (local.get $b))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $l)
      )
    )
  )

  (func $varint_len (param $v i32) (result i32)
    (local $x i32)
    (local $n i32)
    (local.set $x (local.get $v))
    (local.set $n (i32.const 1))
    (block $done
      (loop $l
        (br_if $done (i32.lt_u (local.get $x) (i32.const 128)))
        (local.set $x (i32.shr_u (local.get $x) (i32.const 7)))
        (local.set $n (i32.add (local.get $n) (i32.const 1)))
        (br $l)
      )
    )
    (local.get $n)
  )

  ;; write_varint(dst, v) -> bytes_written
  (func $write_varint (param $dst i32) (param $v i32) (result i32)
    (local $x i32)
    (local $i i32)
    (local $b i32)
    (local.set $x (local.get $v))
    (local.set $i (i32.const 0))
    (block $done
      (loop $l
        (local.set $b (i32.and (local.get $x) (i32.const 0x7f)))
        (local.set $x (i32.shr_u (local.get $x) (i32.const 7)))
        (if (i32.ne (local.get $x) (i32.const 0))
          (then (local.set $b (i32.or (local.get $b) (i32.const 0x80))))
        )
        (i32.store8 (i32.add (local.get $dst) (local.get $i)) (local.get $b))
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br_if $done (i32.eq (local.get $x) (i32.const 0)))
        (br $l)
      )
    )
    (local.get $i)
  )

  ;; check for ASCII prefix "tunnel:" (7 bytes)
  (func $is_tunnel (param $p i32) (param $n i32) (result i32)
    (if (result i32) (i32.lt_u (local.get $n) (i32.const 7))
      (then (i32.const 0))
      (else
        (if (result i32)
          (i32.and
            (i32.eq (i32.load8_u (local.get $p)) (i32.const 0x74))
            (i32.and
              (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 1))) (i32.const 0x75))
              (i32.and
                (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 2))) (i32.const 0x6e))
                (i32.and
                  (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 3))) (i32.const 0x6e))
                  (i32.and
                    (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 4))) (i32.const 0x65))
                    (i32.and
                      (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 5))) (i32.const 0x6c))
                      (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.const 6))) (i32.const 0x3a))
                    )
                  )
                )
              )
            )
          )
          (then (i32.const 1))
          (else (i32.const 0))
        )
      )
    )
  )

  ;; parse port from selected_upstream. Returns i64 { port (low32), ok (high32) }
  (func $parse_port (param $p i32) (param $n i32) (result i64)
    (local $colon i32)
    (local $i i32)
    (local $b i32)
    (local $port i32)
    (local $start i32)

    (local.set $colon (i32.const -1))

    ;; bracketed IPv6: [addr]:port
    (if (i32.eq (i32.load8_u (local.get $p)) (i32.const 0x5b))
      (then
        (local.set $i (i32.const 1))
        (block $br_done
          (loop $br
            (br_if $br_done (i32.ge_u (local.get $i) (local.get $n)))
            (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
            (if (i32.eq (local.get $b) (i32.const 0x5d))
              (then
                ;; require ':' after ']'
                (if (i32.lt_u (i32.add (local.get $i) (i32.const 1)) (local.get $n))
                  (then
                    (if (i32.eq (i32.load8_u (i32.add (local.get $p) (i32.add (local.get $i) (i32.const 1)))) (i32.const 0x3a))
                      (then (local.set $colon (i32.add (local.get $i) (i32.const 1))))
                    )
                  )
                )
                (br $br_done)
              )
            )
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $br)
          )
        )
      )
      (else
        ;; find last ':'
        (local.set $i (i32.sub (local.get $n) (i32.const 1)))
        (block $done
          (loop $l
            (br_if $done (i32.lt_s (local.get $i) (i32.const 0)))
            (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
            (if (i32.eq (local.get $b) (i32.const 0x3a))
              (then
                (local.set $colon (local.get $i))
                (br $done)
              )
            )
            (local.set $i (i32.sub (local.get $i) (i32.const 1)))
            (br $l)
          )
        )
      )
    )

    (if (i32.lt_s (local.get $colon) (i32.const 0))
      (then (return (call $pack (i32.const 0) (i32.const 0))))
    )

    (local.set $start (i32.add (local.get $colon) (i32.const 1)))
    (if (i32.ge_u (local.get $start) (local.get $n))
      (then (return (call $pack (i32.const 0) (i32.const 0))))
    )

    (local.set $port (i32.const 0))
    (local.set $i (local.get $start))
    (block $pdone
      (loop $pl
        (br_if $pdone (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
        (if (i32.or (i32.lt_u (local.get $b) (i32.const 0x30)) (i32.gt_u (local.get $b) (i32.const 0x39)))
          (then (return (call $pack (i32.const 0) (i32.const 0))))
        )
        (local.set $port
          (i32.add
            (i32.mul (local.get $port) (i32.const 10))
            (i32.sub (local.get $b) (i32.const 0x30))
          )
        )
        (if (i32.gt_u (local.get $port) (i32.const 65535))
          (then (return (call $pack (i32.const 0) (i32.const 0))))
        )
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $pl)
      )
    )

    (call $pack (local.get $port) (i32.const 1))
  )

  ;; upstream_host_slice(p,n) -> i64 { host_ptr (low32), host_len (high32) }
  ;; strips :port (if present and numeric), and strips [] for bracketed IPv6.
  (func $upstream_host_slice (param $p i32) (param $n i32) (result i64)
    (local $b i32)
    (local $i i32)
    (local $colon i32)
    (local $digits_ok i32)

    (if (i32.eq (local.get $n) (i32.const 0))
      (then (return (call $pack (i32.const 0) (i32.const 0))))
    )

    ;; [ipv6]:port
    (if (i32.eq (i32.load8_u (local.get $p)) (i32.const 0x5b))
      (then
        (local.set $i (i32.const 1))
        (block $bdone
          (loop $bl
            (br_if $bdone (i32.ge_u (local.get $i) (local.get $n)))
            (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
            (if (i32.eq (local.get $b) (i32.const 0x5d))
              (then
                (return (call $pack (i32.add (local.get $p) (i32.const 1)) (i32.sub (local.get $i) (i32.const 1))))
              )
            )
            (local.set $i (i32.add (local.get $i) (i32.const 1)))
            (br $bl)
          )
        )
        (return (call $pack (i32.const 0) (i32.const 0)))
      )
    )

    ;; find last ':'
    (local.set $colon (i32.const -1))
    (local.set $i (i32.sub (local.get $n) (i32.const 1)))
    (block $done
      (loop $l
        (br_if $done (i32.lt_s (local.get $i) (i32.const 0)))
        (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
        (if (i32.eq (local.get $b) (i32.const 0x3a))
          (then
            (local.set $colon (local.get $i))
            (br $done)
          )
        )
        (local.set $i (i32.sub (local.get $i) (i32.const 1)))
        (br $l)
      )
    )

    (if (i32.lt_s (local.get $colon) (i32.const 0))
      (then (return (call $pack (local.get $p) (local.get $n))))
    )

    ;; ensure suffix is numeric (port)
    (local.set $digits_ok (i32.const 1))
    (local.set $i (i32.add (local.get $colon) (i32.const 1)))
    (block $pd
      (loop $pl
        (br_if $pd (i32.ge_u (local.get $i) (local.get $n)))
        (local.set $b (i32.load8_u (i32.add (local.get $p) (local.get $i))))
        (if (i32.or (i32.lt_u (local.get $b) (i32.const 0x30)) (i32.gt_u (local.get $b) (i32.const 0x39)))
          (then
            (local.set $digits_ok (i32.const 0))
            (br $pd)
          )
        )
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $pl)
      )
    )

    (if (i32.eq (local.get $digits_ok) (i32.const 1))
      (then (return (call $pack (local.get $p) (local.get $colon))))
    )

    (call $pack (local.get $p) (local.get $n))
  )

  (func $set_rewrite (param $ptr i32) (param $len i32) (result i64)
    ;; out struct at 65536: { host_ptr, host_len, rw_ptr, rw_len }
    (i32.store (i32.const 65536) (i32.const 0))
    (i32.store (i32.const 65540) (i32.const 0))
    (i32.store (i32.const 65544) (local.get $ptr))
    (i32.store (i32.const 65548) (local.get $len))
    (call $pack (i32.const 65536) (i32.const 16))
  )

  ;; Try to rewrite a Minecraft handshake prelude.
  (func $try_rewrite_mc (param $n i32) (param $up_ptr i32) (param $up_len i32) (result i64)
    (local $slice i64)
    (local $host_ptr i32)
    (local $host_len i32)
    (local $tmp i64)
    (local $pkt_len i32)
    (local $len_n i32)
    (local $end i32)
    (local $p i32)

    (local $pid i32)
    (local $pid_n i32)

    (local $proto_n i32)

    (local $addr_len_pos i32)
    (local $addr_len i32)
    (local $addr_n i32)
    (local $addr_ptr i32)
    (local $port_pos i32)

    (local $prefix_len i32)
    (local $rest_ptr i32)
    (local $rest_len i32)

    (local $new_addr_len i32)
    (local $new_addr_n i32)
    (local $new_pkt_len i32)
    (local $new_len_n i32)

    (local $out_ptr i32)
    (local $out_p i32)
    (local $w i32)

    (local $port_pack i64)
    (local $port_ok i32)
    (local $port i32)

    ;; parse packet length
    (local.set $tmp (call $read_varint (i32.const 0) (local.get $n)))
    (local.set $pkt_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $len_n) (i32.const 0)) (then (return (i64.const 1))))
    (if (i32.lt_s (local.get $pkt_len) (i32.const 0)) (then (return (i64.const 1))))

    (local.set $end (i32.add (local.get $len_n) (local.get $pkt_len)))
    (if (i32.gt_u (local.get $end) (local.get $n)) (then (return (i64.const 1))))

    (local.set $p (local.get $len_n))

    ;; packet id
    (local.set $tmp (call $read_varint (local.get $p) (local.get $end)))
    (local.set $pid (i32.wrap_i64 (local.get $tmp)))
    (local.set $pid_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $pid_n) (i32.const 0)) (then (return (i64.const 1))))
    (if (i32.ne (local.get $pid) (i32.const 0)) (then (return (i64.const 1))))
    (local.set $p (i32.add (local.get $p) (local.get $pid_n)))

    ;; protocol version
    (local.set $tmp (call $read_varint (local.get $p) (local.get $end)))
    (local.set $proto_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $proto_n) (i32.const 0)) (then (return (i64.const 1))))
    (local.set $p (i32.add (local.get $p) (local.get $proto_n)))

    ;; address length
    (local.set $addr_len_pos (local.get $p))
    (local.set $tmp (call $read_varint (local.get $p) (local.get $end)))
    (local.set $addr_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $addr_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $addr_n) (i32.const 0)) (then (return (i64.const 1))))
    (if (i32.lt_s (local.get $addr_len) (i32.const 0)) (then (return (i64.const 1))))

    (local.set $addr_ptr (i32.add (local.get $p) (local.get $addr_n)))
    (local.set $port_pos (i32.add (local.get $addr_ptr) (local.get $addr_len)))
    (if (i32.gt_u (i32.add (local.get $port_pos) (i32.const 2)) (local.get $end)) (then (return (i64.const 1))))

    (local.set $prefix_len (i32.sub (local.get $addr_len_pos) (local.get $len_n)))
    (local.set $rest_ptr (i32.add (local.get $port_pos) (i32.const 2)))
    (local.set $rest_len (i32.sub (local.get $end) (local.get $rest_ptr)))

    ;; In Minecraft handshake, the port is a separate field.
    ;; Only write the upstream host (no :port) into the server address string.
    (local.set $slice (call $upstream_host_slice (local.get $up_ptr) (local.get $up_len)))
    (local.set $host_ptr (i32.wrap_i64 (local.get $slice)))
    (local.set $host_len (i32.wrap_i64 (i64.shr_u (local.get $slice) (i64.const 32))))
    (if (i32.eq (local.get $host_len) (i32.const 0)) (then (return (i64.const 1))))

    (local.set $new_addr_len (local.get $host_len))
    (local.set $new_addr_n (call $varint_len (local.get $new_addr_len)))
    (local.set $new_pkt_len
      (i32.add
        (i32.add
          (i32.add
            (i32.add (local.get $prefix_len) (local.get $new_addr_n))
            (local.get $new_addr_len)
          )
          (i32.const 2)
        )
        (local.get $rest_len)
      )
    )
    (local.set $new_len_n (call $varint_len (local.get $new_pkt_len)))
    (drop (local.get $new_len_n))

    (local.set $out_ptr (i32.const 131072))

    ;; length prefix
    (local.set $w (call $write_varint (local.get $out_ptr) (local.get $new_pkt_len)))
    (local.set $out_p (i32.add (local.get $out_ptr) (local.get $w)))

    ;; prefix bytes (packet_id + proto_ver)
    (call $memcpy_impl (local.get $out_p) (local.get $len_n) (local.get $prefix_len))
    (local.set $out_p (i32.add (local.get $out_p) (local.get $prefix_len)))

    ;; new addr len varint
    (local.set $w (call $write_varint (local.get $out_p) (local.get $new_addr_len)))
    (local.set $out_p (i32.add (local.get $out_p) (local.get $w)))

    ;; new addr bytes (host only)
    (call $memcpy_impl (local.get $out_p) (local.get $host_ptr) (local.get $host_len))
    (local.set $out_p (i32.add (local.get $out_p) (local.get $host_len)))

    ;; port
    (local.set $port_pack (call $parse_port (local.get $up_ptr) (local.get $up_len)))
    (local.set $port (i32.wrap_i64 (local.get $port_pack)))
    (local.set $port_ok (i32.wrap_i64 (i64.shr_u (local.get $port_pack) (i64.const 32))))
    (if (i32.eq (local.get $port_ok) (i32.const 1))
      (then (call $write_u16be (local.get $out_p) (local.get $port)))
      (else
        (call $memcpy_impl (local.get $out_p) (local.get $port_pos) (i32.const 2))
      )
    )
    (local.set $out_p (i32.add (local.get $out_p) (i32.const 2)))

    ;; rest
    (call $memcpy_impl (local.get $out_p) (local.get $rest_ptr) (local.get $rest_len))
    (local.set $out_p (i32.add (local.get $out_p) (local.get $rest_len)))

    (return (call $set_rewrite (local.get $out_ptr) (i32.sub (local.get $out_p) (local.get $out_ptr))))
  )

  (func (export "prism_mw_run") (param $n i32) (param $ctx i32) (result i64)
    (local $phase i32)
    (local $p i32)
    (local $end i32)

    (local $up_ptr i32)
    (local $up_len i32)

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
    (if (i32.eq (local.get $phase) (i32.const 1))
      (then
        (local.set $up_ptr (i32.load (i32.add (local.get $ctx) (i32.const 8))))
        (local.set $up_len (i32.load (i32.add (local.get $ctx) (i32.const 12))))
        (if (i32.eq (local.get $up_len) (i32.const 0)) (then (return (i64.const 1))))

        ;; don't rewrite tunnel labels
        (if (i32.eq (call $is_tunnel (local.get $up_ptr) (local.get $up_len)) (i32.const 1))
          (then (return (i64.const 1)))
        )

        (return (call $try_rewrite_mc (local.get $n) (local.get $up_ptr) (local.get $up_len)))
      )
    )
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
