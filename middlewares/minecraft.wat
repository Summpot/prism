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

(component
  ;; ---------------------------------------------------------------------------
  ;; Declarative Configuration Schema (Component Type Definition)
  ;; ---------------------------------------------------------------------------
  (type $Config (record
    (field "recompress-threshold" u32)
    (field "deflate-level" u8)
    (field "discovery-targets" string)
    (field "motd-template" string)
  ))
  (export "config" (type $Config))

  (core module $main
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
  ;; Memory (64 pages = 4 MiB)
  ;; ---------------------------------------------------------------------------
  ;; Layout:
  ;;   Page 0 (0..65535): general buffer space / input
  ;;   Page 1 (65536..131071): route match struct at 65536, stream frame struct at 65552
  ;;   Page 2 (131072..196607): scratch / rewrite buffer
  ;;   Page 3 (196608..262143): injected data (e.g. RSA private key) at 196608
  ;;   Page 4+ (262144..): decompressed streaming payload buffer
  (memory (export "memory") 64)

  ;; ---------------------------------------------------------------------------
  ;; Static Data Segments (Page 1: 65600..65800)
  ;; ---------------------------------------------------------------------------
  (data (i32.const 65600) "224.0.2.60:4445,255.255.255.255:4445,127.0.0.1:4445")
  (data (i32.const 65660) "[MOTD]{prefix}{name}[/MOTD][AD]{port}[/AD]")
  (data (i32.const 65710) "[MOTD]")
  (data (i32.const 65720) "[/MOTD][AD]")
  (data (i32.const 65740) "[/AD]")

  ;; ---------------------------------------------------------------------------
  ;; Internal State Globals & Dynamic Configuration
  ;; ---------------------------------------------------------------------------

  (global $injected_len (mut i32) (i32.const 0))
  (global $recompress_threshold (mut i32) (i32.const 256))
  (global $deflate_level (mut i32) (i32.const 1))
  (global $targets_ptr (mut i32) (i32.const 65600))
  (global $targets_len (mut i32) (i32.const 51))
  (global $template_ptr (mut i32) (i32.const 65660))
  (global $template_len (mut i32) (i32.const 42))

  (func $set_recompress_threshold (export "set_recompress_threshold") (param $val i32)
    (global.set $recompress_threshold (local.get $val))
  )

  (func $set_deflate_level (export "set_deflate_level") (param $val i32)
    (global.set $deflate_level (local.get $val))
  )

  (func $set_discovery_targets (export "set_discovery_targets") (param $ptr i32) (param $len i32)
    (if (i32.and (i32.gt_s (local.get $len) (i32.const 0)) (i32.le_s (local.get $len) (i32.const 1024)))
      (then
        (call $memcpy (i32.const 132000) (local.get $ptr) (local.get $len))
        (global.set $targets_ptr (i32.const 132000))
        (global.set $targets_len (local.get $len))
      )
    )
  )

  (func $set_discovery_template (export "set_discovery_template") (export "set_motd_template") (param $ptr i32) (param $len i32)
    (if (i32.and (i32.gt_s (local.get $len) (i32.const 0)) (i32.le_s (local.get $len) (i32.const 1024)))
      (then
        (call $memcpy (i32.const 133024) (local.get $ptr) (local.get $len))
        (global.set $template_ptr (i32.const 133024))
        (global.set $template_len (local.get $len))
      )
    )
  )

  (func (export "update_config")
    (param $threshold i32)
    (param $level i32)
    (param $targets_ptr i32) (param $targets_len i32)
    (param $template_ptr i32) (param $template_len i32)
    (result i32)
    (if (i32.ge_s (local.get $threshold) (i32.const 0))
      (then (call $set_recompress_threshold (local.get $threshold)))
    )
    (if (i32.ge_s (local.get $level) (i32.const 0))
      (then (call $set_deflate_level (local.get $level)))
    )
    (if (i32.gt_s (local.get $targets_len) (i32.const 0))
      (then (call $set_discovery_targets (local.get $targets_ptr) (local.get $targets_len)))
    )
    (if (i32.gt_s (local.get $template_len) (i32.const 0))
      (then (call $set_discovery_template (local.get $template_ptr) (local.get $template_len)))
    )
    (i32.const 0)
  )

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

  ;; write_varint(buf_ptr, val) -> nbytes:i32
  (func $write_varint (param $buf_ptr i32) (param $val i32) (result i32)
    (local $p i32)
    (local $b i32)
    (local $v i32)
    (local.set $p (local.get $buf_ptr))
    (local.set $v (local.get $val))
    (loop $loop
      (local.set $b (i32.and (local.get $v) (i32.const 0x7f)))
      (local.set $v (i32.shr_u (local.get $v) (i32.const 7)))
      (if (i32.ne (local.get $v) (i32.const 0))
        (then
          (local.set $b (i32.or (local.get $b) (i32.const 0x80)))
        )
      )
      (i32.store8 (local.get $p) (local.get $b))
      (local.set $p (i32.add (local.get $p) (i32.const 1)))
      (br_if $loop (i32.ne (local.get $v) (i32.const 0)))
    )
    (i32.sub (local.get $p) (local.get $buf_ptr))
  )

  ;; write_u32_ascii(ptr, val) -> len (writes decimal ASCII representation of val to ptr)
  (func $write_u32_ascii (param $ptr i32) (param $val i32) (result i32)
    (local $temp_ptr i32)
    (local $v i32)
    (local $digit i32)
    (local $len i32)
    (local $i i32)

    (if (i32.eq (local.get $val) (i32.const 0))
      (then
        (i32.store8 (local.get $ptr) (i32.const 48)) ;; '0'
        (return (i32.const 1))
      )
    )

    ;; Write digits backwards into scratch buffer at 131072
    (local.set $temp_ptr (i32.const 131072))
    (local.set $v (local.get $val))
    (local.set $len (i32.const 0))

    (loop $digit_loop
      (local.set $digit (i32.rem_u (local.get $v) (i32.const 10)))
      (i32.store8
        (i32.add (local.get $temp_ptr) (local.get $len))
        (i32.add (local.get $digit) (i32.const 48))
      )
      (local.set $len (i32.add (local.get $len) (i32.const 1)))
      (local.set $v (i32.div_u (local.get $v) (i32.const 10)))
      (br_if $digit_loop (i32.gt_u (local.get $v) (i32.const 0)))
    )

    ;; Reverse digits into $ptr
    (local.set $i (i32.const 0))
    (loop $copy_loop
      (i32.store8
        (i32.add (local.get $ptr) (local.get $i))
        (i32.load8_u
          (i32.sub (i32.sub (i32.add (local.get $temp_ptr) (local.get $len)) (local.get $i)) (i32.const 1))
        )
      )
      (local.set $i (i32.add (local.get $i) (i32.const 1)))
      (br_if $copy_loop (i32.lt_u (local.get $i) (local.get $len)))
    )

    (local.get $len)
  )

  ;; Check whether packet ID represents an urgent packet (KeepAlive or Ping/Pong)
  (func $is_urgent_packet (param $pid i32) (result i32)
    ;; Status Ping / Pong / Request: 0x01
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
    (local $data_len i32)
    (local $data_len_n i32)
    (local $decomp_len i32)
    (local $is_urgent i32)
    (local $tmp_n i32)
    (local $hdr_len i32)
    (local $frame_ptr i32)
    (local $total_payload_len i32)

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

    (local.set $p (i32.add (local.get $buf_ptr) (local.get $len_n)))

    ;; Check for Minecraft Deflate compression (data_length VarInt at $p)
    (local.set $tmp (call $read_varint (local.get $p) (i32.add (local.get $buf_ptr) (local.get $total_len))))
    (local.set $data_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $data_len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    ;; If data_length > 0 and data_len_n > 0: attempt Deflate decompression
    (if (i32.and (i32.gt_s (local.get $data_len) (i32.const 0)) (i32.gt_s (local.get $data_len_n) (i32.const 0)))
      (then
        ;; Call $deflate_decompress:
        ;; in_ptr = $p + $data_len_n
        ;; in_len = $pkt_len - $data_len_n
        ;; out_ptr = 262150 (Page 4 + 6 bytes headroom for [VarInt Length] + [0x00])
        ;; out_max_len = 3145722 (3 MiB - 6)
        (local.set $decomp_len
          (call $deflate_decompress
            (i32.add (local.get $p) (local.get $data_len_n))
            (i32.sub (local.get $pkt_len) (local.get $data_len_n))
            (i32.const 262150)
            (i32.const 3145722)
          )
        )
        (if (i32.gt_s (local.get $decomp_len) (i32.const 0))
          (then
            ;; Uncompressed packet framing: [VarInt(decomp_len + 1)] [0x00] [decompressed_payload]
            ;; Calculate VarInt length for (decomp_len + 1) in scratch at Page 2 (131072)
            (local.set $tmp_n (call $write_varint (i32.const 131072) (i32.add (local.get $decomp_len) (i32.const 1))))
            ;; Total header length = $tmp_n + 1 (for 0x00)
            (local.set $hdr_len (i32.add (local.get $tmp_n) (i32.const 1)))
            (local.set $frame_ptr (i32.sub (i32.const 262150) (local.get $hdr_len)))
            ;; Write VarInt at $frame_ptr
            (drop (call $write_varint (local.get $frame_ptr) (i32.add (local.get $decomp_len) (i32.const 1))))
            ;; Write 0x00 at ($frame_ptr + $tmp_n)
            (i32.store8 (i32.add (local.get $frame_ptr) (local.get $tmp_n)) (i32.const 0))

            ;; Full framed packet length = $hdr_len + $decomp_len
            (local.set $total_payload_len (i32.add (local.get $hdr_len) (local.get $decomp_len)))

            ;; Write struct StreamFrame at fixed offset 65552:
            ;; offset 65552: consumed_len (i32) = $total_len
            ;; offset 65556: payload_ptr (i32) = $frame_ptr
            ;; offset 65560: payload_len (i32) = $total_payload_len
            (i32.store (i32.const 65552) (local.get $total_len))
            (i32.store (i32.const 65556) (local.get $frame_ptr))
            (i32.store (i32.const 65560) (local.get $total_payload_len))

            ;; Decompressed game payloads (chunks, lighting, entity data) must always defer
            ;; to enable full 20ms aggregation batching into continuous Zstd streams.
            (return (call $pack_result (i32.const 3) (i32.const 65552)))
          )
        )
      )
    )

    ;; Fallback / uncompressed packet:
    ;; If data_length was 0, packet ID starts at $p + $data_len_n. Otherwise at $p.
    (if (i32.and (i32.eq (local.get $data_len) (i32.const 0)) (i32.gt_s (local.get $data_len_n) (i32.const 0)))
      (then
        (local.set $p (i32.add (local.get $p) (local.get $data_len_n)))
      )
    )

    (local.set $tmp (call $read_varint (local.get $p) (i32.add (local.get $buf_ptr) (local.get $total_len))))
    (local.set $pid (i32.wrap_i64 (local.get $tmp)))
    (local.set $pid_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    (if (i32.eq (local.get $pid_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 1) (local.get $total_len))))
    )

    (if (call $is_urgent_packet (local.get $pid))
      (then
        (return (call $pack_result (i32.const 2) (local.get $total_len)))
      )
    )

    (call $pack_result (i32.const 1) (local.get $total_len))
  )

  ;; ---------------------------------------------------------------------------
  ;; State 2: Streaming Egress Logic (Tunnel -> Local Socket)
  ;; Recompresses packets when data_length == 0 and uncompressed payload >= 256
  ;; ---------------------------------------------------------------------------

  (func $poll_egress (param $buf_ptr i32) (param $buf_len i32) (result i64)
    (local $tmp i64)
    (local $pkt_len i32)
    (local $len_n i32)
    (local $total_len i32)
    (local $buf_end i32)
    (local $p i32)
    (local $data_len i32)
    (local $data_len_n i32)
    (local $raw_payload_ptr i32)
    (local $raw_payload_len i32)
    (local $comp_len i32)
    (local $dl_varint_n i32)
    (local $inner_pkt_len i32)
    (local $pkt_varint_n i32)
    (local $hdr_len i32)
    (local $frame_ptr i32)
    (local $total_out_len i32)

    ;; Empty buffer: NEED_MORE_DATA (Action 0)
    (if (i32.le_s (local.get $buf_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (local.set $buf_end (i32.add (local.get $buf_ptr) (local.get $buf_len)))

    ;; 1. Parse packet length VarInt
    (local.set $tmp (call $read_varint (local.get $buf_ptr) (local.get $buf_end)))
    (local.set $pkt_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    (if (i32.eq (local.get $len_n) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )
    (if (i32.lt_s (local.get $pkt_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (local.set $total_len (i32.add (local.get $len_n) (local.get $pkt_len)))
    (if (i32.lt_u (local.get $buf_len) (local.get $total_len))
      (then (return (call $pack_result (i32.const 0) (i32.const 0))))
    )

    (if (i32.eq (local.get $pkt_len) (i32.const 0))
      (then (return (call $pack_result (i32.const 1) (local.get $total_len))))
    )

    (local.set $p (i32.add (local.get $buf_ptr) (local.get $len_n)))

    ;; Check data_length VarInt at $p
    (local.set $tmp (call $read_varint (local.get $p) (i32.add (local.get $buf_ptr) (local.get $total_len))))
    (local.set $data_len (i32.wrap_i64 (local.get $tmp)))
    (local.set $data_len_n (i32.wrap_i64 (i64.shr_u (local.get $tmp) (i64.const 32))))

    ;; Only recompress if data_length == 0 and data_len_n > 0
    (if (i32.and (i32.eq (local.get $data_len) (i32.const 0)) (i32.gt_s (local.get $data_len_n) (i32.const 0)))
      (then
        (local.set $raw_payload_ptr (i32.add (local.get $p) (local.get $data_len_n)))
        (local.set $raw_payload_len (i32.sub (local.get $pkt_len) (local.get $data_len_n)))

        ;; Recompress threshold: dynamic threshold from global
        (if (i32.ge_u (local.get $raw_payload_len) (global.get $recompress_threshold))
          (then
            ;; Compress into Page 4 + 16 (262160), leaving 16 bytes headroom for two VarInts
            (local.set $comp_len
              (call $deflate_compress
                (local.get $raw_payload_ptr)
                (local.get $raw_payload_len)
                (i32.const 262160)
                (i32.const 3145712)
                (global.get $deflate_level)
              )
            )
            (if (i32.gt_s (local.get $comp_len) (i32.const 0))
              (then
                ;; Form [Packet Length: VarInt] [Data Length: VarInt(raw_payload_len)] [compressed data]
                ;; 1. Write Data Length VarInt in scratch (131072)
                (local.set $dl_varint_n (call $write_varint (i32.const 131072) (local.get $raw_payload_len)))
                (local.set $inner_pkt_len (i32.add (local.get $dl_varint_n) (local.get $comp_len)))
                ;; 2. Write Packet Length VarInt in scratch (131080)
                (local.set $pkt_varint_n (call $write_varint (i32.const 131080) (local.get $inner_pkt_len)))

                (local.set $hdr_len (i32.add (local.get $pkt_varint_n) (local.get $dl_varint_n)))
                (local.set $frame_ptr (i32.sub (i32.const 262160) (local.get $hdr_len)))

                ;; Copy Packet Length VarInt
                (call $memcpy (local.get $frame_ptr) (i32.const 131080) (local.get $pkt_varint_n))
                ;; Copy Data Length VarInt
                (call $memcpy (i32.add (local.get $frame_ptr) (local.get $pkt_varint_n)) (i32.const 131072) (local.get $dl_varint_n))

                (local.set $total_out_len (i32.add (local.get $hdr_len) (local.get $comp_len)))

                ;; Write struct StreamFrame at 65552:
                ;; offset 65552: consumed_len (i32) = $total_len
                ;; offset 65556: payload_ptr (i32) = $frame_ptr
                ;; offset 65560: payload_len (i32) = $total_out_len
                (i32.store (i32.const 65552) (local.get $total_len))
                (i32.store (i32.const 65556) (local.get $frame_ptr))
                (i32.store (i32.const 65560) (local.get $total_out_len))

                (return (call $pack_result (i32.const 3) (i32.const 65552)))
              )
            )
          )
        )
      )
    )

    ;; Fallback / below threshold: passthrough frame as-is
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

    ;; State 1: Streaming Ingress
    (if (i32.eq (local.get $state) (i32.const 1))
      (then
        (return (call $poll_streaming (local.get $buf_ptr) (local.get $buf_len)))
      )
    )

    ;; State 2: Streaming Egress
    (if (i32.eq (local.get $state) (i32.const 2))
      (then
        (return (call $poll_egress (local.get $buf_ptr) (local.get $buf_len)))
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

  ;; discovery_targets(out_ptr, out_max_len) -> i32
  ;; Writes the discovery target addresses (comma-separated).
  ;; Returns written byte count, or -1 if out_max_len is too small.
  (func (export "discovery_targets")
    (param $out_ptr i32)
    (param $out_max_len i32)
    (result i32)
    (if (i32.lt_s (local.get $out_max_len) (global.get $targets_len))
      (then (return (i32.const -1)))
    )
    (call $memcpy (local.get $out_ptr) (global.get $targets_ptr) (global.get $targets_len))
    (global.get $targets_len)
  )

  ;; discovery_template(out_ptr, out_max_len) -> i32
  ;; Writes the discovery payload template string.
  ;; Returns written byte count, or -1 if out_max_len is too small.
  (func (export "discovery_template")
    (param $out_ptr i32)
    (param $out_max_len i32)
    (result i32)
    (if (i32.lt_s (local.get $out_max_len) (global.get $template_len))
      (then (return (i32.const -1)))
    )
    (call $memcpy (local.get $out_ptr) (global.get $template_ptr) (global.get $template_len))
    (global.get $template_len)
  )

  ;; build_discovery_payload(name_ptr, name_len, port, prefix_ptr, prefix_len, out_ptr, out_max_len) -> i32
  ;; Generates discovery payload: "[MOTD]{prefix}{name}[/MOTD][AD]{port}[/AD]"
  ;; Returns total byte length written to out_ptr, or -1 on error/buffer too small.
  (func (export "build_discovery_payload")
    (param $name_ptr i32) (param $name_len i32)
    (param $port i32)
    (param $prefix_ptr i32) (param $prefix_len i32)
    (param $out_ptr i32) (param $out_max_len i32)
    (result i32)

    (local $curr i32)
    (local $port_len i32)
    (local $needed i32)

    (local.set $needed
      (i32.add
        (i32.add (local.get $prefix_len) (local.get $name_len))
        (i32.const 28)
      )
    )
    (if (i32.lt_s (local.get $out_max_len) (local.get $needed))
      (then (return (i32.const -1)))
    )

    (local.set $curr (local.get $out_ptr))

    ;; 1. "[MOTD]" (6 bytes at 65710)
    (call $memcpy (local.get $curr) (i32.const 65710) (i32.const 6))
    (local.set $curr (i32.add (local.get $curr) (i32.const 6)))

    ;; 2. prefix
    (if (i32.gt_u (local.get $prefix_len) (i32.const 0))
      (then
        (call $memcpy (local.get $curr) (local.get $prefix_ptr) (local.get $prefix_len))
        (local.set $curr (i32.add (local.get $curr) (local.get $prefix_len)))
      )
    )

    ;; 3. name
    (if (i32.gt_u (local.get $name_len) (i32.const 0))
      (then
        (call $memcpy (local.get $curr) (local.get $name_ptr) (local.get $name_len))
        (local.set $curr (i32.add (local.get $curr) (local.get $name_len)))
      )
    )

    ;; 4. "[/MOTD][AD]" (11 bytes at 65720)
    (call $memcpy (local.get $curr) (i32.const 65720) (i32.const 11))
    (local.set $curr (i32.add (local.get $curr) (i32.const 11)))

    ;; 5. port ASCII
    (local.set $port_len (call $write_u32_ascii (local.get $curr) (local.get $port)))
    (local.set $curr (i32.add (local.get $curr) (local.get $port_len)))

    ;; 6. "[/AD]" (5 bytes at 65740)
    (call $memcpy (local.get $curr) (i32.const 65740) (i32.const 5))
    (local.set $curr (i32.add (local.get $curr) (i32.const 5)))

    ;; Return total written bytes
    (i32.sub (local.get $curr) (local.get $out_ptr))
  )
  )

  (export "main" (core module $main))
)
