;; Prism middleware: tls_sni (parse host only)
;;
;; - Parse phase: extracts the first TLS ClientHello SNI hostname as routing host.
;; - Rewrite phase: no-op.

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

  (func $read_u24be (param $p i32) (result i32)
    (i32.or
      (i32.or
        (i32.shl (i32.load8_u (local.get $p)) (i32.const 16))
        (i32.shl (i32.load8_u (i32.add (local.get $p) (i32.const 1))) (i32.const 8))
      )
      (i32.load8_u (i32.add (local.get $p) (i32.const 2)))
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
