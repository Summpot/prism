;; Prism unified WASM protocol driver: minecraft.wat
;;
;; Implements the unified WASM protocol driver for Minecraft:
;; - Memory: 4 pages (256 KiB)
;; - Host Imports: crypto & compression acceleration in namespace "prism"
;; - Exports:
;;   - memory
;;   - poll(buf_ptr, buf_len, state) -> packed i64: (Action << 32) | Value
;;       state == 0 (Handshaking):
;;         Action 0 (NEED_MORE_DATA): buffer incomplete
;;         Action 1 (ROUTE_MATCH): handshake parsed, Value = pointer to struct { host_ptr, host_len, rewrite_ptr, rewrite_len } at 65536
;;         Action 2 (NO_MATCH): format does not match
;;       state == 1 (Streaming):
;;         Action 0 (NEED_MORE_DATA): packet incomplete
;;         Action 1 (FRAME_DEFER): sliced packet (normal game packet), Value = total packet bytes
;;         Action 2 (FRAME_URGENT): sliced packet (KeepAlive / Ping / Pong), Value = total packet bytes
;;   - set_data(ptr, len) -> i32: copies injected data (e.g. RSA private key) to offset 196608, returns 0

(module
  ;; ---------------------------------------------------------------------------
  ;; Host Imports in namespace "prism" (Must precede memories, globals, funcs)
  ;; ---------------------------------------------------------------------------

  ;; RSA PKCS#1 v1.5 private key decryption
  (import "prism" "crypto_rsa_decrypt"
    (func $crypto_rsa_decrypt
      (param $key_ptr i32) (param $key_len i32)
      (param $in_ptr i32) (param $in_len i32)
      (param $out_ptr i32)
      (result i32)
    )
  )

  ;; AES-128-CFB8 in-place encrypt/decrypt
  (import "prism" "crypto_aes_cfb8"
    (func $crypto_aes_cfb8
      (param $key_ptr i32)
      (param $iv_ptr i32)
      (param $data_ptr i32) (param $data_len i32)
      (param $is_encrypt i32)
      (result i32)
    )
  )

  ;; Zlib/Deflate decompression (RFC 1950/1951)
  (import "prism" "deflate_decompress"
    (func $deflate_decompress
      (param $in_ptr i32) (param $in_len i32)
      (param $out_ptr i32) (param $out_max_len i32)
      (result i32)
    )
  )

  ;; Zlib/Deflate compression (RFC 1950/1951)
  (import "prism" "deflate_compress"
    (func $deflate_compress
      (param $in_ptr i32) (param $in_len i32)
      (param $out_ptr i32) (param $out_max_len i32)
      (param $level i32)
      (result i32)
    )
  )

  ;; HPACK dynamic symbol table query/intern
  (import "prism" "sym_intern"
    (func $sym_intern
      (param $str_ptr i32) (param $str_len i32)
      (result i64)
    )
  )

  ;; HPACK dynamic symbol table resolve
  (import "prism" "sym_resolve"
    (func $sym_resolve
      (param $index i32) (param $out_ptr i32) (param $max_len i32)
      (result i32)
    )
  )

  ;; ---------------------------------------------------------------------------
  ;; Memory (4 pages = 256 KiB)
  ;; ---------------------------------------------------------------------------
  ;; Layout:
  ;;   Page 0 (0..65535): general buffer space / input
  ;;   Page 1 (65536..131071): route match struct { host_ptr, host_len, rw_ptr, rw_len } at 65536
  ;;   Page 2 (131072..196607): scratch / rewrite / decompression buffer
  ;;   Page 3 (196608..262143): injected data (e.g. RSA private key) at 196608, length at 196604
  (memory (export "memory") 4)

  ;; ---------------------------------------------------------------------------
  ;; Internal State Globals
  ;; ---------------------------------------------------------------------------

  (global $injected_len (mut i32) (i32.const 0))

  ;; ---------------------------------------------------------------------------
  ;; Utility Functions
  ;; ---------------------------------------------------------------------------

  ;; Pack Action (high 32 bits) and Value (low 32 bits) into i64: (Action << 32) | Value
  (func $pack_result (param $action i32) (param $value i32) (result i64)
    (i64.or
      (i64.shl (i64.extend_i32_u (local.get $action)) (i64.const 32))
      (i64.extend_i32_u (local.get $value))
    )
  )

  ;; Helper to pack VarInt read result: value (low 32 bits) and bytes_read (high 32 bits)
  (func $pack_varint (param $val i32) (param $nbytes i32) (result i64)
    (i64.or
      (i64.extend_i32_u (local.get $val))
      (i64.shl (i64.extend_i32_u (local.get $nbytes)) (i64.const 32))
    )
  )

  ;; read_varint(ptr, end) -> i64 { value:u32 (low 32), nbytes:u32 (high 32) }
  ;; nbytes == 0 indicates incomplete or invalid VarInt
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
        ;; VarInts are at most 5 bytes (35 bits). Shift > 28 implies > 5 bytes.
        (br_if $done (i32.gt_s (local.get $shift) (i32.const 28)))

        (br $loop)
      )
    )

    (if (result i64)
      (i32.and
        (i32.gt_u (local.get $i) (local.get $ptr))
        (i32.eq (i32.and (local.get $b) (i32.const 0x80)) (i32.const 0))
      )
      (then
        (call $pack_varint
          (local.get $res)
          (i32.sub (local.get $i) (local.get $ptr))
        )
      )
      (else
        (call $pack_varint (i32.const 0) (i32.const 0))
      )
    )
  )

  ;; Copy n bytes from src to dst
  (func $memcpy (param $dst i32) (param $src i32) (param $n i32)
    (local $i i32)
    (local.set $i (i32.const 0))
    (block $done
      (loop $loop
        (br_if $done (i32.ge_u (local.get $i) (local.get $n)))
        (i32.store8
          (i32.add (local.get $dst) (local.get $i))
          (i32.load8_u (i32.add (local.get $src) (local.get $i)))
        )
        (local.set $i (i32.add (local.get $i) (i32.const 1)))
        (br $loop)
      )
    )
  )

  ;; Check whether packet ID represents an urgent packet (KeepAlive or Ping/Pong)
  (func $is_urgent_packet (param $pid i32) (result i32)
    ;; Status Ping / Pong / Request / Response: 0x00, 0x01
    (if (i32.eq (local.get $pid) (i32.const 0x00)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x01)) (then (return (i32.const 1))))

    ;; Serverbound KeepAlive across Minecraft versions:
    ;; 0x0B (1.9-1.11.2), 0x0C (1.12-1.12.2), 0x0E (1.13-1.13.2),
    ;; 0x0F (1.14-1.15.2, 1.17-1.18.2), 0x10 (1.16-1.16.5),
    ;; 0x11 (1.19-1.19.3), 0x12 (1.19.4, 1.20-1.20.1),
    ;; 0x14 (1.20.2), 0x15 (1.20.3+)
    (if (i32.eq (local.get $pid) (i32.const 0x0B)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x0C)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x0E)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x0F)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x10)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x11)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x12)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x14)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x15)) (then (return (i32.const 1))))

    ;; Clientbound KeepAlive across Minecraft versions:
    ;; 0x1F (1.9-1.12, 1.16, 1.19.3), 0x20 (1.14, 1.19-1.19.2),
    ;; 0x21 (1.13, 1.15, 1.17-1.18.2), 0x23 (1.19.4, 1.20-1.20.1),
    ;; 0x24 (1.20.3-1.20.4), 0x26 (1.20.2, 1.20.5+)
    (if (i32.eq (local.get $pid) (i32.const 0x1F)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x20)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x21)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x23)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x24)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x26)) (then (return (i32.const 1))))

    ;; Play Ping / Pong (1.17+): 0x1D, 0x1E, 0x30, 0x31, 0x32
    (if (i32.eq (local.get $pid) (i32.const 0x1D)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x1E)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x30)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x31)) (then (return (i32.const 1))))
    (if (i32.eq (local.get $pid) (i32.const 0x32)) (then (return (i32.const 1))))

    (i32.const 0)
  )

  ;; ---------------------------------------------------------------------------
  ;; State 0: Handshaking Logic
  ;; ---------------------------------------------------------------------------

  (func $poll_handshake (param $buf_ptr i32) (param $buf_len i32) (result i64)
    (local $tmp i64)
    (local $pkt_len i32)
    (local $len_n i32)
    (local $buf_end i32)
    (local $pkt_end i32)
    (local $p i32)
    (local $pid i32)
    (local $pid_n i32)
    (local $proto_n i32)
    (local $addr_len i32)
    (local $addr_n i32)
    (local $addr_ptr i32)
    (local $host_len i32)
    (local $i i32)
    (local $b i32)

    ;; Empty buffer: NEED_MORE_DATA (Action 0)
    (if (i32.le_s (local.get $buf_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (local.set $buf_end (i32.add (local.get $buf_ptr) (local.get $buf_len)))

    ;; 1. Parse packet length VarInt
    (local.set $tmp (call $read_varint (local.get $buf_ptr) (local.get $buf_end)))
    (local.set $pkt_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    ;; VarInt incomplete: NEED_MORE_DATA (Action 0)
    (if (i32.eq (local.get $len_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    ;; Packet length must be positive
    (if (i32.le_s (local.get $pkt_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    ;; Check if full packet is in buffer
    (if (i32.lt_u (local.get $buf_len) (i32.add (local.get $len_n) (local.get $pkt_len)))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (local.set $p (i32.add (local.get $buf_ptr) (local.get $len_n)))
    (local.set $pkt_end (i32.add (local.get $p) (local.get $pkt_len)))

    ;; 2. Read packet ID (must be 0x00 for handshake)
    (local.set $tmp (call $read_varint (local.get $p) (local.get $pkt_end)))
    (local.set $pid (i32.wrap_i64 (local.get $tmp)))
    (local.set $pid_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    (if (i32.eq (local.get $pid_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (if (i32.ne (local.get $pid) (i32.const 0))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $p (i32.add (local.get $p) (local.get $pid_n)))

    ;; 3. Read protocol version VarInt
    (local.set $tmp (call $read_varint (local.get $p) (local.get $pkt_end)))
    (local.set $proto_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))
    (if (i32.eq (local.get $proto_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (local.set $p (i32.add (local.get $p) (local.get $proto_n)))

    ;; 4. Read server address string length VarInt
    (local.set $tmp (call $read_varint (local.get $p) (local.get $pkt_end)))
    (local.set $addr_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $addr_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    (if (i32.eq (local.get $addr_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )
    (if (i32.le_s (local.get $addr_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    (local.set $addr_ptr (i32.add (local.get $p) (local.get $addr_n)))

    ;; Must fit address bytes + port (2 bytes)
    (if (i32.gt_u
          (i32.add (i32.add (local.get $addr_ptr) (local.get $addr_len)) (i32.const 2))
          (local.get $pkt_end)
        )
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    ;; 5. Scan address for NUL byte (preserve Forge/Bungee/Velocity forwarding data)
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

    ;; If host prefix is empty: NO_MATCH (Action 2)
    (if (i32.eq (local.get $host_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 2) (i32.const 0))))
    )

    ;; 6. Store struct at fixed memory location 65536:
    ;;    { host_ptr: i32, host_len: i32, rewrite_ptr: i32, rewrite_len: i32 }
    (i32.store (i32.const 65536) (local.get $addr_ptr))
    (i32.store (i32.const 65540) (local.get $host_len))
    (i32.store (i32.const 65544) (i32.const 0))
    (i32.store (i32.const 65548) (i32.const 0))

    ;; ROUTE_MATCH (Action 1), Value = 65536 (pointer to struct)
    (call $pack_result (i32.const 1) (i32.const 65536))
  )

  ;; ---------------------------------------------------------------------------
  ;; State 1: Streaming Logic
  ;; ---------------------------------------------------------------------------

  (func $poll_streaming (param $buf_ptr i32) (param $buf_len i32) (result i64)
    (local $tmp i64)
    (local $pkt_len i32)
    (local $len_n i32)
    (local $total_len i32)
    (local $buf_end i32)
    (local $p i32)
    (local $pid i32)
    (local $pid_n i32)

    ;; Empty buffer: NEED_MORE_DATA (Action 0)
    (if (i32.le_s (local.get $buf_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (local.set $buf_end (i32.add (local.get $buf_ptr) (local.get $buf_len)))

    ;; 1. Parse packet length VarInt
    (local.set $tmp (call $read_varint (local.get $buf_ptr) (local.get $buf_end)))
    (local.set $pkt_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    ;; VarInt incomplete: NEED_MORE_DATA (Action 0)
    (if (i32.eq (local.get $len_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    ;; Invalid negative packet length: NEED_MORE_DATA (Action 0)
    (if (i32.lt_s (local.get $pkt_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (local.set $total_len (i32.add (local.get $len_n) (local.get $pkt_len)))

    ;; Buffer does not contain complete packet yet: NEED_MORE_DATA (Action 0)
    (if (i32.lt_u (local.get $buf_len) (local.get $total_len))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    ;; Full packet sliced! Total packet bytes = varint_len + packet_length
    ;; If packet payload is empty (pkt_len == 0): FRAME_DEFER
    (if (i32.eq (local.get $pkt_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 1) (local.get $total_len))))
    )

    ;; 2. Read packet ID
    (local.set $p (i32.add (local.get $buf_ptr) (local.get $len_n)))
    (local.set $tmp (call $read_varint (local.get $p) (i32.add (local.get $buf_ptr) (local.get $total_len))))
    (local.set $pid (i32.wrap_i64 (local.get $tmp)))
    (local.set $pid_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    ;; If packet ID cannot be parsed, defer packet
    (if (i32.eq (local.get $pid_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 1) (local.get $total_len))))
    )

    ;; 3. Check for urgent packet (KeepAlive or Ping/Pong)
    (if (call $is_urgent_packet (local.get $pid))
      (then
        ;; FRAME_URGENT (Action 2), Value = total packet bytes
        (return (call $pack_result (i32.const 2) (local.get $total_len)))
      )
    )

    ;; Normal game packet: FRAME_DEFER (Action 1), Value = total packet bytes
    (call $pack_result (i32.const 1) (local.get $total_len))
  )

  ;; ---------------------------------------------------------------------------
  ;; Exported Functions
  ;; ---------------------------------------------------------------------------

  ;; poll(buf_ptr, buf_len, state) -> i64
  ;; Packed return: (Action << 32) | Value
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

  ;; set_data(ptr, len) -> i32
  ;; Injects arbitrary data (e.g. RSA private key DER) into internal buffer at offset 196608 (page 3).
  ;; Records length at memory offset 196604 and internal global $injected_len.
  ;; Returns 0 on success, -1 on invalid arguments.
  (func (export "set_data")
    (param $ptr i32)
    (param $len i32)
    (result i32)

    (if (i32.lt_s (local.get $len) (i32.const 0))
      (then (return (i32.const -1)))
    )

    ;; Max length for page 3: 65536 bytes
    (if (i32.gt_u (local.get $len) (i32.const 65536))
      (then (return (i32.const -1)))
    )

    (global.set $injected_len (local.get $len))
    (i32.store (i32.const 196604) (local.get $len))

    (if (i32.gt_u (local.get $len) (i32.const 0))
      (then
        (call $memcpy (i32.const 196608) (local.get $ptr) (local.get $len))
      )
    )

    (i32.const 0)
  )
)
