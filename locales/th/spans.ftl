# ส่วนสแปน: รายการสแปน/เทรซต่อโปรเจกต์ (spans-*) และหน้ารายละเอียด
# waterfall ของเทรซ (trace-detail-*) ใช้ nav-spans ข้อความที่นับจำนวนใช้พหูพจน์ tv_count

# --- ส่วนต่อท้ายชื่อหน้า ---
spans-title-suffix = — Stackpit

# --- รายการสแปน/เทรซ ---
spans-list-empty = ไม่พบสแปนสำหรับโปรเจกต์นี้
spans-traces-heading = เทรซ
spans-all-heading = สแปนทั้งหมด

# --- ตารางเทรซ ---
spans-col-trace-id = รหัสเทรซ
spans-col-root-op = Op ราก
spans-col-root-description = คำอธิบายราก
spans-col-duration = ระยะเวลา
spans-col-first-seen = พบครั้งแรก
spans-col-last-seen = พบล่าสุด

# --- ตารางสแปนทั้งหมด ---
spans-col-span-id = รหัสสแปน
spans-col-op = Op
spans-col-description = คำอธิบาย
spans-col-timestamp = ประทับเวลา

# --- การแบ่งหน้า (รายการสแปน) ---
spans-pagination-label = การแบ่งหน้า
spans-pagination-prev = « ก่อนหน้า
spans-pagination-next = ถัดไป »
spans-count = { $count ->
   *[other] { $count } สแปน
}

# --- รายละเอียดเทรซ (waterfall) ---
# title-prefix/suffix ห่อรหัสเทรซแบบไดนามิก total/showing-first/of ถูกแยกที่
# ขอบเขต { $var } ของบรรทัดข้อมูลย่อย
trace-detail-title-prefix = เทรซ
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = ทั้งหมด
trace-detail-showing-first = แสดงรายการแรก
trace-detail-of = จาก
trace-detail-empty = ไม่พบสแปนสำหรับเทรซนี้
trace-detail-col-span = สแปน
trace-detail-col-duration = ระยะเวลา
trace-detail-root-fallback = (รากของเทรซ)
trace-detail-error-title = ข้อผิดพลาด
trace-detail-span-fallback = สแปน
trace-detail-compressed-note = บีบอัดช่วงว่าง
trace-detail-gap-title = ช่วงว่างที่ยุบรวม (ไม่มีสแปนที่ทำงาน)
trace-detail-lbl-span-id = รหัสสแปน
trace-detail-lbl-parent = สแปนแม่
trace-detail-lbl-status = สถานะ
trace-detail-lbl-start = ออฟเซ็ตเริ่มต้น
trace-detail-correlated-errors = ข้อผิดพลาดที่เกี่ยวข้อง
trace-detail-col-level = ระดับ
trace-detail-col-title = หัวข้อ
trace-detail-col-timestamp = ประทับเวลา
trace-detail-span-count = { $count ->
   *[other] { $count } สแปน
}
