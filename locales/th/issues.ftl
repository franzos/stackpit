# ส่วนปัญหา: รายการปัญหาที่จัดกลุ่มตามลายนิ้วมือและหน้ารายละเอียดปัญหา
# issue-detail-exception-stacktrace มี &amp; แบบอินไลน์และเรนเดอร์ด้วย |safe
# ข้อความที่นับจำนวนใช้พหูพจน์ tv_count

# --- ป้ายกำกับร่วม (รายการปัญหา + รายละเอียดปัญหา) ---
issues-label-title = หัวข้อ
issues-label-level = ระดับ
issues-label-events = เหตุการณ์
issues-label-users = ผู้ใช้
issues-label-trend = แนวโน้ม
issues-trend-tooltip = ปริมาณเหตุการณ์ในช่วงเวลาที่เลือก
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
issues-list-filter-environment = กรองตามสภาพแวดล้อม
issues-list-environment-all = ทุกสภาพแวดล้อม
issues-period-label = ช่วงเวลา
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
issue-detail-in-app-only = เฉพาะเฟรมของแอป
issue-detail-reverse-order = สลับลำดับ
issue-detail-copy = คัดลอก
issue-detail-copy-frame = คัดลอกเฟรมนี้
issue-detail-library-frames = { $count ->
   *[other] { $count } เฟรมจากไลบรารี
}
issue-detail-minified-hint = เฟรมเหล่านี้ดูเหมือนถูกย่อขนาดและไม่มีการใช้ source map
issue-detail-minified-hint-link = อัปโหลด source map
issue-detail-breadcrumbs = เบรดครัมบ์
issue-detail-th-time = เวลา
issue-detail-th-category = หมวดหมู่
issue-detail-th-message = ข้อความ
issue-detail-crumb-data = ข้อมูล
issue-detail-crumb-filter = กรองเบรดครัมบ์ตามประเภท
issue-detail-crumb-filter-all = ทุกประเภท
issue-detail-tags = แท็ก
issue-detail-contexts = บริบท
issue-detail-additional-data = ข้อมูลเพิ่มเติม
issue-detail-view-replay = ดูรีเพลย์
issue-detail-view-trace = ดูเทรซ
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
issue-detail-create-external-issue = สร้างปัญหา
issue-detail-external-tracker = ตัวติดตามภายนอก
issue-detail-view-on = ดูที่
flash-tracker-create-failed = สร้างปัญหาในตัวติดตามไม่สำเร็จ ตรวจสอบโทเคนและที่เก็บโค้ดของการเชื่อมต่อ แล้วลองอีกครั้ง
flash-tracker-config-incomplete = การเชื่อมต่อตัวติดตามนี้ขาดที่เก็บโค้ดหรือโทเคน แก้ไขได้ในการตั้งค่าการเชื่อมต่อ
issue-detail-external-unlink = ยกเลิกการเชื่อมโยง
issue-detail-external-unlink-confirm = ลบการเชื่อมโยงนี้หรือไม่ ปัญหายังคงอยู่บนฟอร์จ ให้ปิดหรือลบที่นั่น
issue-detail-external-orphaned = ลบการเชื่อมต่อแล้ว
flash-tracker-unlinked = ลบการเชื่อมโยงแล้ว ปัญหายังคงอยู่บนฟอร์จ
flash-tracker-ambiguous = โปรเจกต์นี้มีที่เก็บโค้ดมากกว่าหนึ่งแห่งที่ตัวติดตามนี้สร้างปัญหาได้ เลือกหนึ่งแห่งแล้วลองอีกครั้ง
issue-detail-crumbs-truncated = { $count ->
   *[other] แสดง { $count } รายการล่าสุด
}
issue-detail-crumbs-show-all = { $count ->
   *[other] แสดงทั้งหมด { $count }
}
issue-detail-external-state-open = เปิด
issue-detail-external-state-closed = ปิด
