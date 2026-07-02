# ส่วนมอนิเตอร์: รายการมอนิเตอร์ (การเช็คอินแบบ cron) ต่อโปรเจกต์และหน้า
# รายละเอียดมอนิเตอร์ ใช้ nav-monitors ข้อความที่นับจำนวนใช้พหูพจน์ tv_count

# --- ส่วนต่อท้ายชื่อหน้า ---
monitors-title-suffix = — Stackpit

# --- รายการมอนิเตอร์ ---
monitors-list-empty = ไม่พบมอนิเตอร์ เหตุการณ์เช็คอินที่มี <code class="text-mono">monitor_slug</code> จะปรากฏที่นี่
monitors-col-slug = Slug
monitors-col-last-status = สถานะล่าสุด
monitors-col-last-checkin = เช็คอินล่าสุด
monitors-col-count = จำนวน

# --- รายละเอียดมอนิเตอร์ ---
monitors-detail-title-prefix = มอนิเตอร์
monitors-detail-subtitle = การเช็คอินของมอนิเตอร์
monitors-detail-empty = ไม่พบการเช็คอินสำหรับมอนิเตอร์นี้
monitors-detail-select-checkin = เลือกการเช็คอิน
monitors-detail-confirm-delete-selected = ลบการเช็คอินที่เลือกหรือไม่
monitors-detail-delete = ลบ
monitors-detail-col-title = หัวข้อ
monitors-detail-col-level = ระดับ
monitors-detail-col-environment = สภาพแวดล้อม
monitors-detail-col-time = เวลา
monitors-detail-untitled = (ไม่มีชื่อ)
monitors-detail-confirm-delete-all = { $count ->
   *[other] ลบการเช็คอินทั้งหมด { $count } รายการหรือไม่
}
monitors-detail-delete-all = { $count ->
   *[other] ลบทั้งหมด { $count } รายการ
}

# --- การแบ่งหน้า ---
monitors-pagination-label = การแบ่งหน้า
monitors-pagination-prev = « ก่อนหน้า
monitors-pagination-next = ถัดไป »
monitors-detail-count = { $count ->
   *[other] { $count } การเช็คอิน
}
