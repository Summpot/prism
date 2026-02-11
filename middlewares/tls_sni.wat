;; Prism middleware: tls_sni (parse + rewrite)
;;
;; - Parse phase: extracts the first TLS ClientHello SNI hostname as routing host.
;; - Rewrite phase: rewrites the first ClientHello SNI hostname to the selected upstream host
;;   (port is stripped from selected_upstream).

(module
  (memory (export "memory") 4)

  (func $pack (param $ptr i32) (param $len i32) (result i64)
    (i64.or
      (i64.extend_i32_u (local.get $ptr))
      (i64.shl (i64.extend_i32_u (local.get $len)) (i64.const 32))
    )
  )

  (func $read_u16be (param $p i32) (result i32)
    (i32.or
      (i32.shl (i32.load8_u (local.get $p)) (i32.const 8))
      (i32.load8_u (i32.add (local.get $p) (i32.const 1)))
    )
  )

  (func $write_u16be (param $p i32) (param $v i32)
    (i32.store8 (local.get $p) (i32.and (i32.shr_u (local.get $v) (i32.const 8)) (i32.const 0xff)))
    (i32.store8 (i32.add (local.get $p) (i32.const 1)) (i32.and (local.get $v) (i32.const 0xff)))
  )

  (func $read_u24be (param $p i32) (result i32)
    (i32.or
      (i32.or
        (i32.shl (i32.load8_u (local.get $p)) (i32.const 16))
        (i32.shl (i32.load8_u (i32.add (local.get $p) (i32.const 1))) (i32.const 8))
      )
      (i32.load8_u (i32.add (local.get $p) (i32.const 2)))
    )
  )

  (func $write_u24be (param $p i32) (param $v i32)
    (i32.store8 (local.get $p) (i32.and (i32.shr_u (local.get $v) (i32.const 16)) (i32.const 0xff)))
    (i32.store8 (i32.add (local.get $p) (i32.const 1)) (i32.and (i32.shr_u (local.get $v) (i32.const 8)) (i32.const 0xff)))
    (i32.store8 (i32.add (local.get $p) (i32.const 2)) (i32.and (local.get $v) (i32.const 0xff)))
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

  ;; Try to rewrite TLS ClientHello SNI.
  (func $try_rewrite_tls (param $n i32) (param $up_ptr i32) (param $up_len i32) (result i64)
    (local $slice i64)
    (local $new_ptr i32)
    (local $new_len i32)

    (local $ct i32)
    (local $rec_len i32)
    (local $rec_end i32)

    (local $hs_type i32)
    (local $hs_len i32)
    (local $ch_end i32)

    (local $p i32)
    (local $sid_len i32)
    (local $cs_len i32)
    (local $cm_len i32)

    (local $ext_total_off i32)
    (local $ext_total i32)
    (local $ext_end i32)

    (local $ext_hdr i32)
    (local $ext_type i32)
    (local $ext_len i32)
    (local $ext_len_off i32)
    (local $ext_data i32)
    (local $ext_data_end i32)

    (local $list_len i32)
    (local $list_len_off i32)
    (local $q i32)
    (local $list_end i32)
    (local $name_type i32)
    (local $name_len i32)
    (local $name_len_off i32)
    (local $host_ptr i32)

    (local $delta i32)
    (local $out_ptr i32)
    (local $tail_src i32)
    (local $tail_len i32)

    ;; upstream host slice (strip :port)
    (local.set $slice (call $upstream_host_slice (local.get $up_ptr) (local.get $up_len)))
    (local.set $new_ptr (i32.wrap_i64 (local.get $slice)))
    (local.set $new_len (i32.wrap_i64 (i64.shr_u (local.get $slice) (i64.const 32))))
    (if (i32.eq (local.get $new_len) (i32.const 0)) (then (return (i64.const 1))))

    (if (i32.lt_u (local.get $n) (i32.const 5)) (then (return (i64.const 1))))
    (local.set $ct (i32.load8_u (i32.const 0)))
    (if (i32.ne (local.get $ct) (i32.const 22)) (then (return (i64.const 1))))

    (local.set $rec_len (call $read_u16be (i32.const 3)))
    (local.set $rec_end (i32.add (i32.const 5) (local.get $rec_len)))
    (if (i32.gt_u (local.get $rec_end) (local.get $n)) (then (return (i64.const 1))))

    (local.set $hs_type (i32.load8_u (i32.const 5)))
    (if (i32.ne (local.get $hs_type) (i32.const 1)) (then (return (i64.const 1))))

    (local.set $hs_len (call $read_u24be (i32.const 6)))
    (local.set $ch_end (i32.add (i32.const 9) (local.get $hs_len)))
    (if (i32.gt_u (local.get $ch_end) (local.get $rec_end)) (then (return (i64.const 1))))

    (local.set $p (i32.const 9))
    (if (i32.gt_u (i32.add (local.get $p) (i32.const 34)) (local.get $ch_end)) (then (return (i64.const 1))))
    (local.set $p (i32.add (local.get $p) (i32.const 34)))

    ;; session id
    (local.set $sid_len (i32.load8_u (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 1)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $sid_len)) (local.get $ch_end)) (then (return (i64.const 1))))
    (local.set $p (i32.add (local.get $p) (local.get $sid_len)))

    ;; cipher suites
    (local.set $cs_len (call $read_u16be (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 2)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $cs_len)) (local.get $ch_end)) (then (return (i64.const 1))))
    (local.set $p (i32.add (local.get $p) (local.get $cs_len)))

    ;; compression methods
    (local.set $cm_len (i32.load8_u (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 1)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $cm_len)) (local.get $ch_end)) (then (return (i64.const 1))))
    (local.set $p (i32.add (local.get $p) (local.get $cm_len)))

    ;; extensions
    (local.set $ext_total_off (local.get $p))
    (local.set $ext_total (call $read_u16be (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 2)))
    (local.set $ext_end (i32.add (local.get $p) (local.get $ext_total)))
    (if (i32.gt_u (local.get $ext_end) (local.get $ch_end)) (then (return (i64.const 1))))

    (block $no_sni
      (loop $ext_loop
        (br_if $no_sni (i32.gt_u (i32.add (local.get $p) (i32.const 4)) (local.get $ext_end)))

        (local.set $ext_hdr (local.get $p))
        (local.set $ext_type (call $read_u16be (local.get $p)))
        (local.set $ext_len (call $read_u16be (i32.add (local.get $p) (i32.const 2))))
        (local.set $ext_len_off (i32.add (local.get $ext_hdr) (i32.const 2)))
        (local.set $p (i32.add (local.get $p) (i32.const 4)))
        (local.set $ext_data (local.get $p))
        (local.set $ext_data_end (i32.add (local.get $p) (local.get $ext_len)))
        (if (i32.gt_u (local.get $ext_data_end) (local.get $ext_end)) (then (return (i64.const 1))))

        (if (i32.eq (local.get $ext_type) (i32.const 0))
          (then
            (local.set $list_len_off (local.get $ext_data))
            (local.set $list_len (call $read_u16be (local.get $ext_data)))
            (local.set $q (i32.add (local.get $ext_data) (i32.const 2)))
            (local.set $list_end (i32.add (local.get $q) (local.get $list_len)))
            (if (i32.gt_u (local.get $list_end) (local.get $ext_data_end)) (then (return (i64.const 1))))

            (block $no_name
              (loop $name_loop
                (br_if $no_name (i32.gt_u (i32.add (local.get $q) (i32.const 3)) (local.get $list_end)))

                (local.set $name_type (i32.load8_u (local.get $q)))
                (local.set $name_len_off (i32.add (local.get $q) (i32.const 1)))
                (local.set $name_len (call $read_u16be (local.get $name_len_off)))
                (local.set $host_ptr (i32.add (local.get $q) (i32.const 3)))
                (if (i32.gt_u (i32.add (local.get $host_ptr) (local.get $name_len)) (local.get $list_end)) (then (return (i64.const 1))))

                (if (i32.eq (local.get $name_type) (i32.const 0))
                  (then
                    ;; delta and output build
                    (local.set $delta (i32.sub (local.get $new_len) (local.get $name_len)))
                    (local.set $out_ptr (i32.const 131072))

                    ;; copy head
                    (call $memcpy_impl (local.get $out_ptr) (i32.const 0) (local.get $host_ptr))
                    ;; new host
                    (call $memcpy_impl (i32.add (local.get $out_ptr) (local.get $host_ptr)) (local.get $new_ptr) (local.get $new_len))

                    (local.set $tail_src (i32.add (local.get $host_ptr) (local.get $name_len)))
                    (local.set $tail_len (i32.sub (local.get $n) (local.get $tail_src)))
                    (call $memcpy_impl
                      (i32.add (i32.add (local.get $out_ptr) (local.get $host_ptr)) (local.get $new_len))
                      (local.get $tail_src)
                      (local.get $tail_len)
                    )

                    ;; patch lengths (all offsets are before host_ptr)
                    (call $write_u16be (i32.add (local.get $out_ptr) (i32.const 3)) (i32.add (local.get $rec_len) (local.get $delta)))
                    (call $write_u24be (i32.add (local.get $out_ptr) (i32.const 6)) (i32.add (local.get $hs_len) (local.get $delta)))
                    (call $write_u16be (i32.add (local.get $out_ptr) (local.get $ext_total_off)) (i32.add (local.get $ext_total) (local.get $delta)))
                    (call $write_u16be (i32.add (local.get $out_ptr) (local.get $ext_len_off)) (i32.add (local.get $ext_len) (local.get $delta)))
                    (call $write_u16be (i32.add (local.get $out_ptr) (local.get $list_len_off)) (i32.add (local.get $list_len) (local.get $delta)))
                    (call $write_u16be (i32.add (local.get $out_ptr) (local.get $name_len_off)) (local.get $new_len))

                    (return (call $set_rewrite (local.get $out_ptr) (i32.add (local.get $n) (local.get $delta))))
                  )
                )

                ;; next name
                (local.set $q (i32.add (local.get $host_ptr) (local.get $name_len)))
                (br $name_loop)
              )
            )

            (br $no_sni)
          )
        )

        (local.set $p (local.get $ext_data_end))
        (br $ext_loop)
      )
    )

    (i64.const 1)
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
    (local $up_ptr i32)
    (local $up_len i32)
    (local $ct i32)
    (local $rec_len i32)
    (local $rec_end i32)

    (local $hs_type i32)
    (local $hs_len i32)
    (local $ch_end i32)

    (local $p i32)
    (local $sid_len i32)
    (local $cs_len i32)
    (local $cm_len i32)

    (local $ext_total i32)
    (local $ext_end i32)
    (local $ext_type i32)
    (local $ext_len i32)
    (local $ext_data_end i32)

    (local $list_len i32)
    (local $q i32)
    (local $list_end i32)
    (local $name_type i32)
    (local $name_len i32)

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

        (return (call $try_rewrite_tls (local.get $n) (local.get $up_ptr) (local.get $up_len)))
      )
    )
    (if (i32.ne (local.get $phase) (i32.const 0))
      (then (return (i64.const 1)))
    )

    (if (i32.lt_u (local.get $n) (i32.const 5))
      (then (return (i64.const 0)))
    )

    (local.set $ct (i32.load8_u (i32.const 0)))
    (if (i32.ne (local.get $ct) (i32.const 22))
      (then (return (i64.const 1)))
    )

    (local.set $rec_len (call $read_u16be (i32.const 3)))
    (local.set $rec_end (i32.add (i32.const 5) (local.get $rec_len)))
    (if (i32.gt_u (local.get $rec_end) (local.get $n))
      (then (return (i64.const 0)))
    )

    ;; handshake header
    (if (i32.lt_u (local.get $rec_len) (i32.const 4))
      (then (return (i64.const 1)))
    )

    (local.set $hs_type (i32.load8_u (i32.const 5)))
    (if (i32.ne (local.get $hs_type) (i32.const 1))
      (then (return (i64.const 1)))
    )

    (local.set $hs_len (call $read_u24be (i32.const 6)))
    (local.set $ch_end (i32.add (i32.const 9) (local.get $hs_len)))
    (if (i32.gt_u (local.get $ch_end) (local.get $rec_end))
      (then (return (i64.const 0)))
    )

    ;; clienthello
    (local.set $p (i32.const 9))
    (if (i32.gt_u (i32.add (local.get $p) (i32.const 34)) (local.get $ch_end))
      (then (return (i64.const 0)))
    )
    (local.set $p (i32.add (local.get $p) (i32.const 34)))

    ;; session id
    (if (i32.ge_u (local.get $p) (local.get $ch_end))
      (then (return (i64.const 0)))
    )
    (local.set $sid_len (i32.load8_u (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 1)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $sid_len)) (local.get $ch_end))
      (then (return (i64.const 0)))
    )
    (local.set $p (i32.add (local.get $p) (local.get $sid_len)))

    ;; cipher suites
    (if (i32.gt_u (i32.add (local.get $p) (i32.const 2)) (local.get $ch_end))
      (then (return (i64.const 0)))
    )
    (local.set $cs_len (call $read_u16be (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 2)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $cs_len)) (local.get $ch_end))
      (then (return (i64.const 0)))
    )
    (local.set $p (i32.add (local.get $p) (local.get $cs_len)))

    ;; compression methods
    (if (i32.ge_u (local.get $p) (local.get $ch_end))
      (then (return (i64.const 0)))
    )
    (local.set $cm_len (i32.load8_u (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 1)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $cm_len)) (local.get $ch_end))
      (then (return (i64.const 0)))
    )
    (local.set $p (i32.add (local.get $p) (local.get $cm_len)))

    ;; extensions
    (if (i32.gt_u (i32.add (local.get $p) (i32.const 2)) (local.get $ch_end))
      (then (return (i64.const 1)))
    )
    (local.set $ext_total (call $read_u16be (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 2)))
    (local.set $ext_end (i32.add (local.get $p) (local.get $ext_total)))
    (if (i32.gt_u (local.get $ext_end) (local.get $ch_end))
      (then (return (i64.const 0)))
    )

    (block $no_sni
      (loop $ext_loop
        (br_if $no_sni (i32.gt_u (i32.add (local.get $p) (i32.const 4)) (local.get $ext_end)))

        (local.set $ext_type (call $read_u16be (local.get $p)))
        (local.set $ext_len (call $read_u16be (i32.add (local.get $p) (i32.const 2))))
        (local.set $p (i32.add (local.get $p) (i32.const 4)))
        (local.set $ext_data_end (i32.add (local.get $p) (local.get $ext_len)))
        (if (i32.gt_u (local.get $ext_data_end) (local.get $ext_end))
          (then (return (i64.const 0)))
        )

        (if (i32.eq (local.get $ext_type) (i32.const 0))
          (then
            ;; server_name ext
            (if (i32.gt_u (i32.add (local.get $p) (i32.const 2)) (local.get $ext_data_end))
              (then (return (i64.const 0)))
            )
            (local.set $list_len (call $read_u16be (local.get $p)))
            (local.set $q (i32.add (local.get $p) (i32.const 2)))
            (local.set $list_end (i32.add (local.get $q) (local.get $list_len)))
            (if (i32.gt_u (local.get $list_end) (local.get $ext_data_end))
              (then (return (i64.const 0)))
            )

            (block $no_name
              (loop $name_loop
                (br_if $no_name (i32.gt_u (i32.add (local.get $q) (i32.const 3)) (local.get $list_end)))

                (local.set $name_type (i32.load8_u (local.get $q)))
                (local.set $name_len (call $read_u16be (i32.add (local.get $q) (i32.const 1))))
                (local.set $q (i32.add (local.get $q) (i32.const 3)))
                (if (i32.gt_u (i32.add (local.get $q) (local.get $name_len)) (local.get $list_end))
                  (then (return (i64.const 0)))
                )

                (if (i32.eq (local.get $name_type) (i32.const 0))
                  (then
                    (return (call $write_out_host (local.get $q) (local.get $name_len)))
                  )
                )

                (local.set $q (i32.add (local.get $q) (local.get $name_len)))
                (br $name_loop)
              )
            )

            (br $no_sni)
          )
        )

        (local.set $p (local.get $ext_data_end))
        (br $ext_loop)
      )
    )

    (return (i64.const 1))
  )
)
