# ส่วนปัญหา: รายการปัญหาที่จัดกลุ่มตามลายนิ้วมือและหน้ารายละเอียดปัญหา
# issue-detail-exception-stacktrace มี &amp; แบบอินไลน์และเรนเดอร์ด้วย |safe
# ข้อความที่นับจำนวนใช้พหูพจน์ tv_count

# --- ป้ายกำกับร่วม (รายการปัญหา + รายละเอียดปัญหา) ---
issues-label-title = หัวข้อ
issues-label-level = ระดับ
issues-label-events = เหตุการณ์
issues-label-users = ผู้ใช้
issues-label-status = สถานะ
issues-label-first-seen = พบครั้งแรก
issues-label-last-seen = พบล่าสุด
issues-label-value = ค่า

# --- ค่าสถานะ (ตัวเลือกตัวกรอง + ป้าย) ---
issues-status-unresolved = ยังไม่แก้ไข
issues-status-resolved = แก้ไขแล้ว
issues-status-ignored = เพิกเฉย

# --- การแบ่งหน้า (ร่วม) ---
issues-pagination-label = การแบ่งหน้า
issues-pagination-prev = « ก่อนหน้า
issues-pagination-next = ถัดไป »

# --- ส่วนต่อท้ายชื่อหน้า (ชื่อที่มีคำนำหน้าแบบไดนามิก) ---
issues-title-suffix = — Stackpit

# --- รายการปัญหา ---
issues-list-subtitle = ปัญหาที่จัดกลุ่มตามลายนิ้วมือ
issues-list-filtered-by-tag = กรองตามแท็ก:
issues-list-clear-tag = ล้างตัวกรองแท็ก
issues-list-search-placeholder = ค้นหาปัญหา…
issues-list-search-label = ค้นหาปัญหา
issues-list-select = เลือกปัญหา
issues-list-filter-status = กรองตามสถานะ
issues-list-status-all = ทุกสถานะ
issues-list-filter-level = กรองตามระดับ
issues-list-level-all = ทุกระดับ
issues-list-filter-release = กรองตามรีลีส
issues-list-release-all = ทุกรีลีส
issues-period-label = ช่วงเวลา
issues-period-all = ทั้งหมด
issues-period-1h = ชั่วโมงที่ผ่านมา
issues-period-24h = 24 ชม. ที่ผ่านมา
issues-period-7d = 7 วันที่ผ่านมา
issues-period-14d = 14 วันที่ผ่านมา
issues-period-30d = 30 วันที่ผ่านมา
issues-period-90d = 90 วันที่ผ่านมา
issues-period-365d = 365 วันที่ผ่านมา
issues-list-filter-submit = กรอง
issues-list-empty = ไม่มีปัญหาที่ตรงกับตัวกรองปัจจุบัน
issues-untitled = (ไม่มีชื่อ)

# --- การกระทำแบบกลุ่ม ---
issues-bulk-resolve-all = แก้ไขทั้งหมด { $count } รายการ
issues-bulk-ignore-all = เพิกเฉยทั้งหมด { $count } รายการ
issues-bulk-delete-all = ลบทั้งหมด { $count } รายการ
issues-bulk-resolve-confirm = { $count ->
   *[other] ทำเครื่องหมายว่าแก้ไขแล้วสำหรับปัญหาที่ตรงกันทั้งหมด { $count } รายการหรือไม่
}
issues-bulk-ignore-confirm = { $count ->
   *[other] เพิกเฉยปัญหาที่ตรงกันทั้งหมด { $count } รายการหรือไม่
}
issues-bulk-delete-all-confirm = { $count ->
   *[other] ลบปัญหาที่ตรงกันทั้งหมด { $count } รายการอย่างถาวรหรือไม่
}
issues-bulk-resolve = แก้ไข
issues-bulk-ignore = เพิกเฉย
issues-bulk-delete = ลบ
issues-bulk-delete-selected-confirm = ลบปัญหาที่เลือกอย่างถาวรหรือไม่

# --- จำนวน (การแบ่งหน้า) ---
issues-count = { $count ->
   *[other] { $count } ปัญหา
}

# --- รายละเอียดปัญหา ---
issue-detail-title-fallback = ปัญหา
issue-detail-resolve = ✓ แก้ไข
issue-detail-reopen = เปิดใหม่
issue-detail-unignore = ยกเลิกการเพิกเฉย
issue-detail-tab-details = รายละเอียด
issue-detail-tab-events = เหตุการณ์ทั้งหมด
issue-detail-exception-stacktrace = ข้อยกเว้น &amp; Stacktrace
issue-detail-handled = จัดการแล้ว
issue-detail-unhandled = ไม่ได้จัดการ
issue-detail-in = ใน
issue-detail-var-name = ตัวแปร
issue-detail-no-source = ไม่มีบริบทของซอร์สโค้ด
issue-detail-breadcrumbs = เบรดครัมบ์
issue-detail-th-time = เวลา
issue-detail-th-category = หมวดหมู่
issue-detail-th-message = ข้อความ
issue-detail-crumb-data = ข้อมูล
issue-detail-tags = แท็ก
issue-detail-contexts = บริบท
issue-detail-request = คำขอ
issue-detail-headers = เฮดเดอร์
issue-detail-th-header = เฮดเดอร์
issue-detail-query-string = คิวรีสตริง
issue-detail-body = เนื้อหา
issue-detail-environment = สภาพแวดล้อม
issue-detail-user-reports = รายงานจากผู้ใช้
issue-detail-anonymous = ไม่ระบุตัวตน
issue-detail-attachments = ไฟล์แนบ
issue-detail-att-filename = ชื่อไฟล์
issue-detail-att-type = ประเภท
issue-detail-att-size = ขนาด
issue-detail-download = ดาวน์โหลด
issue-detail-raw-json = JSON ดิบ
issue-detail-no-events = ไม่พบเหตุการณ์สำหรับปัญหานี้
issue-detail-ev-id = รหัสเหตุการณ์
issue-detail-ev-timestamp = ประทับเวลา
issue-detail-ev-platform = แพลตฟอร์ม
issue-detail-events-count = { $count ->
   *[other] { $count } เหตุการณ์
}
issue-detail-props-heading = คุณสมบัติของปัญหา
issue-detail-fingerprint = ลายนิ้วมือ
issue-detail-tag-facets = มุมมองแท็ก
issue-detail-discard-undo-title = กลับมารับเหตุการณ์ในอนาคตที่มีลายนิ้วมือนี้
issue-detail-discard-undo = เลิกทิ้ง
issue-detail-discard-confirm = ทิ้งเหตุการณ์ในอนาคตทั้งหมดที่มีลายนิ้วมือนี้หรือไม่
issue-detail-discard-title = ทิ้งเหตุการณ์ในอนาคตที่ตรงกับลายนิ้วมือนี้อย่างเงียบ ๆ
issue-detail-discard = ทิ้งเหตุการณ์ในอนาคต
