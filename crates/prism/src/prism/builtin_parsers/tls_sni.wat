;; Prism builtin WASM routing parser: tls_sni
;;
;; Implements DESIGN.md "WASM Routing Parser ABI (v1)".
;; Exports:
;;   - memory
;;   - prism_parse(input_len:i32) -> i64
;;
;; Parsing behavior matches the Go builtin `TLSSNIHostParser` (single-record, single-handshake parsing).
;; It extracts the first host_name from the TLS ClientHello server_name extension.

(module
  (memory (export "memory") 1)

  (func $pack (param $ptr i32) (param $len i32) (result i64)
    (i64.or
      (i64.extend_i32_u (local.get $ptr))
      (i64.shl (i64.extend_i32_u (local.get $len)) (i64.const 32))
    )
  )

  (func $u16be (param $p i32) (result i32)
    (i32.or
      (i32.shl (i32.load8_u (local.get $p)) (i32.const 8))
      (i32.load8_u (i32.add (local.get $p) (i32.const 1)))
    )
  )

  (func (export "prism_parse") (param $n i32) (result i64)
    (local $recLen i32)
    (local $recStart i32)
    (local $hsLen i32)
    (local $chStart i32)
    (local $i i32)

    (local $sidLen i32)
    (local $csLen i32)
    (local $cmLen i32)

    (local $extLen i32)
    (local $extStart i32)
    (local $j i32)

    (local $extType i32)
    (local $extDataLen i32)
    (local $dataStart i32)

    (local $listLen i32)
    (local $k i32)
    (local $end i32)
    (local $nameType i32)
    (local $nameLen i32)

    (block $ret (result i64)
      ;; TLS record header needs 5 bytes.
      (if (i32.lt_u (local.get $n) (i32.const 5)) (then (br $ret (i64.const 0))))

      ;; type must be handshake (0x16)
      (if (i32.ne (i32.load8_u (i32.const 0)) (i32.const 0x16)) (then (br $ret (i64.const 1))))

      ;; record version major must be 0x03 and minor in [1..4]
      (if (i32.ne (i32.load8_u (i32.const 1)) (i32.const 0x03)) (then (br $ret (i64.const 1))))
      (local.set $i (i32.load8_u (i32.const 2)))
      (if (i32.lt_u (local.get $i) (i32.const 0x01)) (then (br $ret (i64.const 1))))
      (if (i32.gt_u (local.get $i) (i32.const 0x04)) (then (br $ret (i64.const 1))))

      ;; record length
      (local.set $recLen (call $u16be (i32.const 3)))
      (if (i32.le_s (local.get $recLen) (i32.const 0)) (then (br $ret (i64.const 1))))

      (local.set $recStart (i32.const 5))
      (if (i32.lt_u (local.get $n) (i32.add (local.get $recStart) (local.get $recLen)))
        (then (br $ret (i64.const 0)))
      )

      ;; handshake header (4 bytes) must fit in record
      (if (i32.lt_u (local.get $recLen) (i32.const 4)) (then (br $ret (i64.const 0))))

      ;; handshake type must be client_hello (0x01)
      (if (i32.ne (i32.load8_u (local.get $recStart)) (i32.const 0x01)) (then (br $ret (i64.const 1))))

      ;; handshake length (3 bytes)
      (local.set $hsLen
        (i32.or
          (i32.shl (i32.load8_u (i32.add (local.get $recStart) (i32.const 1))) (i32.const 16))
          (i32.or
            (i32.shl (i32.load8_u (i32.add (local.get $recStart) (i32.const 2))) (i32.const 8))
            (i32.load8_u (i32.add (local.get $recStart) (i32.const 3)))
          )
        )
      )
      (if (i32.le_s (local.get $hsLen) (i32.const 0)) (then (br $ret (i64.const 1))))

      ;; require handshake bytes present in this record (Go implementation returns need-more)
      (if (i32.lt_u (local.get $recLen) (i32.add (i32.const 4) (local.get $hsLen)))
        (then (br $ret (i64.const 0)))
      )

      (local.set $chStart (i32.add (local.get $recStart) (i32.const 4)))

      ;; client_version(2) + random(32)
      (if (i32.lt_u (local.get $hsLen) (i32.const 34)) (then (br $ret (i64.const 0))))
      (local.set $i (i32.const 34))

      ;; session_id
      (if (i32.ge_u (local.get $i) (local.get $hsLen)) (then (br $ret (i64.const 0))))
      (local.set $sidLen (i32.load8_u (i32.add (local.get $chStart) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (if (i32.gt_u (i32.add (local.get $i) (local.get $sidLen)) (local.get $hsLen))
        (then (br $ret (i64.const 0)))
      )
      (local.set $i (i32.add (local.get $i) (local.get $sidLen)))

      ;; cipher_suites
      (if (i32.gt_u (i32.add (local.get $i) (i32.const 2)) (local.get $hsLen)) (then (br $ret (i64.const 0))))
      (local.set $csLen (call $u16be (i32.add (local.get $chStart) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 2)))
      (if (i32.lt_s (local.get $csLen) (i32.const 2)) (then (br $ret (i64.const 1))))
      (if (i32.ne (i32.and (local.get $csLen) (i32.const 1)) (i32.const 0)) (then (br $ret (i64.const 1))))
      (if (i32.gt_u (i32.add (local.get $i) (local.get $csLen)) (local.get $hsLen))
        (then (br $ret (i64.const 0)))
      )
      (local.set $i (i32.add (local.get $i) (local.get $csLen)))

      ;; compression_methods
      (if (i32.ge_u (local.get $i) (local.get $hsLen)) (then (br $ret (i64.const 0))))
      (local.set $cmLen (i32.load8_u (i32.add (local.get $chStart) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (if (i32.gt_u (i32.add (local.get $i) (local.get $cmLen)) (local.get $hsLen))
        (then (br $ret (i64.const 0)))
      )
      (local.set $i (i32.add (local.get $i) (local.get $cmLen)))

      ;; extensions
      (if (i32.eq (local.get $i) (local.get $hsLen)) (then (br $ret (i64.const 1))))
      (if (i32.gt_u (i32.add (local.get $i) (i32.const 2)) (local.get $hsLen)) (then (br $ret (i64.const 0))))
      (local.set $extLen (call $u16be (i32.add (local.get $chStart) (local.get $i))))
      (local.set $i (i32.add (local.get $i) (i32.const 2)))
      (if (i32.gt_u (i32.add (local.get $i) (local.get $extLen)) (local.get $hsLen))
        (then (br $ret (i64.const 0)))
      )

      (local.set $extStart (i32.add (local.get $chStart) (local.get $i)))
      (local.set $j (i32.const 0))

      (block $ext_done
        (loop $ext_loop
          ;; while j+4 <= extLen
          (if (i32.gt_u (i32.add (local.get $j) (i32.const 4)) (local.get $extLen))
            (then (br $ext_done))
          )

          (local.set $extType (call $u16be (i32.add (local.get $extStart) (local.get $j))))
          (local.set $extDataLen (call $u16be (i32.add (i32.add (local.get $extStart) (local.get $j)) (i32.const 2))))
          (local.set $j (i32.add (local.get $j) (i32.const 4)))

          (if (i32.gt_u (i32.add (local.get $j) (local.get $extDataLen)) (local.get $extLen))
            (then (br $ret (i64.const 1)))
          )

          ;; Only care about server_name extension type 0
          (if (i32.ne (local.get $extType) (i32.const 0))
            (then
              (local.set $j (i32.add (local.get $j) (local.get $extDataLen)))
              (br $ext_loop)
            )
          )

          (local.set $dataStart (i32.add (local.get $extStart) (local.get $j)))
          ;; data needs at least 2 bytes for listLen
          (if (i32.lt_u (local.get $extDataLen) (i32.const 2)) (then (br $ret (i64.const 1))))

          (local.set $listLen (call $u16be (local.get $dataStart)))
          (if (i32.gt_u (i32.add (i32.const 2) (local.get $listLen)) (local.get $extDataLen))
            (then (br $ret (i64.const 1)))
          )

          (local.set $k (i32.const 2))
          (local.set $end (i32.add (i32.const 2) (local.get $listLen)))

          (block $name_done
            (loop $name_loop
              ;; while k+3 <= end
              (if (i32.gt_u (i32.add (local.get $k) (i32.const 3)) (local.get $end))
                (then (br $name_done))
              )

              (local.set $nameType (i32.load8_u (i32.add (local.get $dataStart) (local.get $k))))
              (local.set $nameLen (call $u16be (i32.add (i32.add (local.get $dataStart) (local.get $k)) (i32.const 1))))
              (local.set $k (i32.add (local.get $k) (i32.const 3)))

              (if (i32.gt_u (i32.add (local.get $k) (local.get $nameLen)) (local.get $end))
                (then (br $ret (i64.const 1)))
              )

              (if (i32.eq (local.get $nameType) (i32.const 0))
                (then
                  (if (i32.eqz (local.get $nameLen)) (then (br $ret (i64.const 1))))
                  (br $ret (call $pack (i32.add (local.get $dataStart) (local.get $k)) (local.get $nameLen)))
                )
              )

              (local.set $k (i32.add (local.get $k) (local.get $nameLen)))
              (br $name_loop)
            )
          )

          ;; No host_name in list => no match
          (br $ret (i64.const 1))
        )
      )

      ;; No server_name extension => no match
      (br $ret (i64.const 1))
    )
  )
)
