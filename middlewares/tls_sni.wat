;; Prism unified WASM protocol driver: tls_sni.wat
;;
;; Implements the unified WASM protocol driver for TLS SNI routing:
;; - Memory: 4 pages (256 KiB)
;; - Exports:
;;   - memory
;;   - poll(buf_ptr, buf_len, state) -> packed i64: (Action << 32) | Value
;;       state == 0 (Handshaking):
;;         Action 0 (NEED_MORE_DATA): TLS record incomplete
;;         Action 1 (ROUTE_MATCH): SNI hostname extracted, Value = pointer to struct { host_ptr, host_len, rewrite_ptr, rewrite_len } at 65536
;;         Action 2 (NO_MATCH): not a ClientHello or no SNI found
;;       state == 1 (Streaming):
;;         Action 0 (NEED_MORE_DATA): TLS record incomplete
;;         Action 1 (FRAME_DEFER): sliced TLS record frame, Value = total record bytes

(module
  (memory (export "memory") 4)

  ;; ---------------------------------------------------------------------------
  ;; Helper: Pack Action (high 32 bits) and Value (low 32 bits) into i64
  ;; ---------------------------------------------------------------------------
  (func $pack_result (param $action i32) (param $val i32) (result i64)
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $action)) (i64.const 32))
      (i64.extend_i32_u (local.get $val))
    )
  )

  ;; ---------------------------------------------------------------------------
  ;; Helper: Big-endian integer readers
  ;; ---------------------------------------------------------------------------
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

  ;; ---------------------------------------------------------------------------
  ;; State 0: Handshake Logic (ClientHello SNI Parsing)
  ;; ---------------------------------------------------------------------------
  (func $poll_handshake (param $buf_ptr i32) (param $buf_len i32) (result i64)
    (local $ct i32)
    (local $rec_len i32)
    (local $total_rec i32)
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
    (local $ext_data i32)
    (local $ext_data_end i32)
    (local $list_len i32)
    (local $q i32)
    (local $list_end i32)
    (local $name_type i32)
    (local $name_len i32)
    (local $name_ptr i32)

    ;; 1. Check TLS record header (5 bytes)
    (if (i32.lt_u (local.get $buf_len) (i32.const 5))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    ;; ContentType must be Handshake (22)
    (local.set $ct (i32.load8_u (local.get $buf_ptr)))
    (if (i32.ne (local.get $ct) (i32.const 22))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    (local.set $rec_len (call $read_u16be (i32.add (local.get $buf_ptr) (i32.const 3))))
    (local.set $total_rec (i32.add (i32.const 5) (local.get $rec_len)))

    ;; Check if full TLS record is available
    (if (i32.lt_u (local.get $buf_len) (local.get $total_rec))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    ;; 2. Check Handshake header (4 bytes: type (1) + length (3))
    ;; Total minimum bytes to read handshake header = 9
    (if (i32.lt_u (local.get $total_rec) (i32.const 9))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    ;; HandshakeType must be ClientHello (1)
    (local.set $hs_type (i32.load8_u (i32.add (local.get $buf_ptr) (i32.const 5))))
    (if (i32.ne (local.get $hs_type) (i32.const 1))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    (local.set $hs_len (call $read_u24be (i32.add (local.get $buf_ptr) (i32.const 6))))
    (local.set $ch_end (i32.add (i32.add (local.get $buf_ptr) (i32.const 9)) (local.get $hs_len)))
    (if (i32.gt_u (local.get $ch_end) (i32.add (local.get $buf_ptr) (local.get $total_rec)))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    ;; 3. Parse ClientHello structure
    ;; Offset starts after Handshake header: buf_ptr + 9
    (local.set $p (i32.add (local.get $buf_ptr) (i32.const 9)))

    ;; Skip client_version (2) + random (32) = 34 bytes
    (if (i32.gt_u (i32.add (local.get $p) (i32.const 34)) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $p (i32.add (local.get $p) (i32.const 34)))

    ;; Session ID
    (if (i32.ge_u (local.get $p) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $sid_len (i32.load8_u (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 1)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $sid_len)) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $p (i32.add (local.get $p) (local.get $sid_len)))

    ;; Cipher Suites
    (if (i32.gt_u (i32.add (local.get $p) (i32.const 2)) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $cs_len (call $read_u16be (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 2)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $cs_len)) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $p (i32.add (local.get $p) (local.get $cs_len)))

    ;; Compression Methods
    (if (i32.ge_u (local.get $p) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $cm_len (i32.load8_u (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 1)))
    (if (i32.gt_u (i32.add (local.get $p) (local.get $cm_len)) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $p (i32.add (local.get $p) (local.get $cm_len)))

    ;; Extensions
    ;; If at end, there are no extensions -> NO_MATCH
    (if (i32.ge_u (local.get $p) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (if (i32.gt_u (i32.add (local.get $p) (i32.const 2)) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $ext_total (call $read_u16be (local.get $p)))
    (local.set $p (i32.add (local.get $p) (i32.const 2)))
    (local.set $ext_end (i32.add (local.get $p) (local.get $ext_total)))
    (if (i32.gt_u (local.get $ext_end) (local.get $ch_end))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    ;; Loop through extensions looking for server_name (type 0)
    (block $scan_ext_done
      (loop $ext_loop
        (br_if $scan_ext_done (i32.gt_u (i32.add (local.get $p) (i32.const 4)) (local.get $ext_end)))

        (local.set $ext_type (call $read_u16be (local.get $p)))
        (local.set $ext_len (call $read_u16be (i32.add (local.get $p) (i32.const 2))))
        (local.set $ext_data (i32.add (local.get $p) (i32.const 4)))
        (local.set $ext_data_end (i32.add (local.get $ext_data) (local.get $ext_len)))

        (if (i32.gt_u (local.get $ext_data_end) (local.get $ext_end))
          (then (return (call $pack_result (i32.const 2) (i32.const 0))))
        )

        ;; Check if extension is server_name (0)
        (if (i32.eq (local.get $ext_type) (i32.const 0))
          (then
            (if (i32.gt_u (i32.add (local.get $ext_data) (i32.const 2)) (local.get $ext_data_end))
              (then (return (call $pack_result (i32.const 2) (i32.const 0))))
            )
            (local.set $list_len (call $read_u16be (local.get $ext_data)))
            (local.set $q (i32.add (local.get $ext_data) (i32.const 2)))
            (local.set $list_end (i32.add (local.get $q) (local.get $list_len)))
            (if (i32.gt_u (local.get $list_end) (local.get $ext_data_end))
              (then (return (call $pack_result (i32.const 2) (i32.const 0))))
            )

            ;; Loop through ServerNameList looking for host_name (type 0)
            (block $scan_names_done
              (loop $names_loop
                (br_if $scan_names_done (i32.gt_u (i32.add (local.get $q) (i32.const 3)) (local.get $list_end)))

                (local.set $name_type (i32.load8_u (local.get $q)))
                (local.set $name_len (call $read_u16be (i32.add (local.get $q) (i32.const 1))))
                (local.set $name_ptr (i32.add (local.get $q) (i32.const 3)))

                (if (i32.gt_u (i32.add (local.get $name_ptr) (local.get $name_len)) (local.get $list_end))
                  (then (return (call $pack_result (i32.const 2) (i32.const 0))))
                )

                (if (i32.eq (local.get $name_type) (i32.const 0))
                  (then
                    (if (i32.gt_u (local.get $name_len) (i32.const 0))
                      (then
                        ;; Store struct at 65536:
                        ;; { host_ptr: i32, host_len: i32, rewrite_ptr: i32, rewrite_len: i32 }
                        (i32.store (i32.const 65536) (local.get $name_ptr))
                        (i32.store (i32.const 65540) (local.get $name_len))
                        (i32.store (i32.const 65544) (i32.const 0))
                        (i32.store (i32.const 65548) (i32.const 0))
                        ;; ROUTE_MATCH (Action 1), Value = 65536
                        (return (call $pack_result (i32.const 1) (i32.const 65536)))
                      )
                    )
                  )
                )

                (local.set $q (i32.add (local.get $name_ptr) (local.get $name_len)))
                (br $names_loop)
              )
            )

            ;; If server_name extension had no valid host_name, no match
            (return (call $pack_result (i32.const 2) (i32.const 0)))
          )
        )

        (local.set $p (local.get $ext_data_end))
        (br $ext_loop)
      )
    )

    ;; No SNI extension found: NO_MATCH (Action 2)
    (call $pack_result (i32.const 2) (i32.const 0))
  )

  ;; ---------------------------------------------------------------------------
  ;; State 1: Streaming Logic (TLS Record Framing)
  ;; ---------------------------------------------------------------------------
  (func $poll_streaming (param $buf_ptr i32) (param $buf_len i32) (result i64)
    (local $rec_len i32)
    (local $total_rec i32)

    ;; Empty or incomplete record header (5 bytes): NEED_MORE_DATA (Action 0)
    (if (i32.lt_u (local.get $buf_len) (i32.const 5))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (local.set $rec_len (call $read_u16be (i32.add (local.get $buf_ptr) (i32.const 3))))
    (local.set $total_rec (i32.add (i32.const 5) (local.get $rec_len)))

    ;; Incomplete TLS record: NEED_MORE_DATA (Action 0)
    (if (i32.lt_u (local.get $buf_len) (local.get $total_rec))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    ;; Sliced complete TLS record frame: FRAME_DEFER (Action 1), Value = total_rec
    (call $pack_result (i32.const 1) (local.get $total_rec))
  )

  ;; ---------------------------------------------------------------------------
  ;; Exported poll function
  ;; ---------------------------------------------------------------------------
  (func (export "poll")
    (param $buf_ptr i32)
    (param $buf_len i32)
    (param $state i32)
    (result i64)

    ;; State 0: Handshaking
    (if (i32.eq (local.get $state) (i32.const 0))
      (then
        (return (call $poll_handshake (local.get $buf_ptr) (local.get $buf_len)))
      )
    )

    ;; State 1: Streaming
    (if (i32.eq (local.get $state) (i32.const 1))
      (then
        (return (call $poll_streaming (local.get $buf_ptr) (local.get $buf_len)))
      )
    )

    ;; Unknown state: NO_MATCH (Action 2)
    (call $pack_result (i32.const 2) (i32.const 0))
  )
)
