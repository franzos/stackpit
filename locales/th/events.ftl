# ส่วนเหตุการณ์: รายการเหตุการณ์ข้ามโปรเจกต์และหน้ารายละเอียดเหตุการณ์
# event-detail-exception-stacktrace มี &amp; แบบอินไลน์และเรนเดอร์ด้วย |safe
# ข้อความที่นับจำนวนใช้พหูพจน์ tv_count

# --- ป้ายกำกับร่วม (รายการเหตุการณ์ + รายละเอียดเหตุการณ์) ---
events-label-title = หัวข้อ
events-label-type = ประเภท
events-label-level = ระดับ
events-label-platform = แพลตฟอร์ม
events-label-environment = สภาพแวดล้อม
events-label-time = เวลา
events-label-value = ค่า

# --- การแบ่งหน้า (ร่วม) ---
events-pagination-label = การแบ่งหน้า
events-pagination-prev = « ก่อนหน้า
events-pagination-next = ถัดไป »

# --- ส่วนต่อท้ายชื่อหน้า (ชื่อที่มีคำนำหน้าแบบไดนามิก) ---
events-title-suffix = — Stackpit

# --- รายการเหตุการณ์ ---
events-list-title = เหตุการณ์ — Stackpit
events-heading = เหตุการณ์
events-list-search-placeholder = ค้นหาเหตุการณ์…
events-list-search-label = ค้นหาเหตุการณ์
events-list-select = เลือกเหตุการณ์
events-list-filter-level = กรองตามระดับ
events-list-level-all = ทุกระดับ
events-list-filter-type = กรองตามประเภท
events-list-type-all = ทุกประเภท
events-list-project-placeholder = รหัสโปรเจกต์
events-list-filter-project = กรองตามโปรเจกต์
events-list-filter-submit = กรอง
events-list-empty = ไม่มีเหตุการณ์ที่ตรงกับตัวกรองปัจจุบัน
events-untitled = (ไม่มีชื่อ)
events-col-project = โปรเจกต์

# --- การกระทำแบบกลุ่ม ---
events-bulk-delete = ลบ
events-bulk-delete-selected-confirm = ลบเหตุการณ์ที่เลือกหรือไม่
events-bulk-delete-all = ลบทั้งหมด { $count } รายการที่ตรงกัน
events-bulk-delete-all-confirm = { $count ->
   *[other] ลบเหตุการณ์ที่ตรงกันทั้งหมด { $count } รายการอย่างถาวรหรือไม่
}

# --- จำนวน (การแบ่งหน้า) ---
events-count = { $count ->
   *[other] { $count } เหตุการณ์
}

# --- รายละเอียดเหตุการณ์ ---
event-detail-event = เหตุการณ์
event-detail-event-id-label = event_id:
event-detail-nav-label = การนำทางเหตุการณ์
event-detail-nav-newer = « ใหม่กว่า
event-detail-nav-older = เก่ากว่า »
event-detail-nav-count = { $count ->
   *[other] { $count } เหตุการณ์
}
event-detail-nav-in-issue = ในปัญหา
event-detail-user-feedback = ความคิดเห็นจากผู้ใช้
event-detail-anonymous = ไม่ระบุตัวตน
event-detail-related-event = เหตุการณ์ที่เกี่ยวข้อง:
event-detail-exception-stacktrace = ข้อยกเว้น &amp; Stacktrace
event-detail-handled = จัดการแล้ว
event-detail-unhandled = ไม่ได้จัดการ
event-detail-in = ใน
event-detail-var-name = ตัวแปร
event-detail-no-source = ไม่มีบริบทของซอร์สโค้ด
event-detail-breadcrumbs = เบรดครัมบ์
event-detail-th-category = หมวดหมู่
event-detail-th-message = ข้อความ
event-detail-tags = แท็ก
event-detail-contexts = บริบท
event-detail-request = คำขอ
event-detail-headers = เฮดเดอร์
event-detail-th-header = เฮดเดอร์
event-detail-query-string = คิวรีสตริง
event-detail-body = เนื้อหา
event-detail-user-reports = รายงานจากผู้ใช้
event-detail-attachments = ไฟล์แนบ
event-detail-att-filename = ชื่อไฟล์
event-detail-att-size = ขนาด
event-detail-download = ดาวน์โหลด
event-detail-web-vitals = Web Vitals
event-detail-raw-json = JSON ดิบ
event-detail-props-heading = คุณสมบัติของเหตุการณ์
event-detail-prop-event-id = รหัสเหตุการณ์
event-detail-prop-timestamp = ประทับเวลา
event-detail-prop-transaction = ทรานแซกชัน
event-detail-prop-release = รีลีส
event-detail-prop-server = เซิร์ฟเวอร์
event-detail-prop-sdk = SDK
event-detail-prop-received = ได้รับเมื่อ
event-detail-user-heading = ผู้ใช้
event-detail-user-id = ID
event-detail-user-email = อีเมล
event-detail-user-username = ชื่อผู้ใช้
event-detail-user-ip = ที่อยู่ IP

# --- รายงานจากไคลเอนต์ (เหตุการณ์ที่ถูกทิ้ง) ---
# ใช้ events-untitled และ events-pagination-* (ร่วม ไฟล์เดียวกัน)
client-reports-title = รายงานจากไคลเอนต์
client-reports-heading = รายงานจากไคลเอนต์
client-reports-dropped-heading = เหตุการณ์ที่ถูกทิ้ง
client-reports-dropped-subtitle = สิ่งที่ SDK ทิ้งก่อนส่ง แยกตามหมวดหมู่และเหตุผล
client-reports-th-category = หมวดหมู่
client-reports-th-reason = เหตุผล
client-reports-th-reasons = เหตุผล
client-reports-th-dropped = ถูกทิ้ง
client-reports-empty = ไม่พบรายงานจากไคลเอนต์สำหรับโปรเจกต์นี้
client-reports-reports-heading = รายงาน
client-reports-delete = ลบ
client-reports-delete-selected-confirm = ลบรายงานที่เลือกหรือไม่
client-reports-th-event-id = รหัสเหตุการณ์
client-reports-th-title = หัวข้อ
client-reports-th-timestamp = ประทับเวลา
client-reports-th-platform = แพลตฟอร์ม
client-reports-th-release = รีลีส
client-reports-select = เลือกรายงาน
client-reports-delete-all = ลบทั้งหมด { $count } รายการ
client-reports-delete-all-confirm = { $count ->
   *[other] ลบรายงานที่ตรงกันทั้งหมด { $count } รายการหรือไม่
}
client-reports-count = { $count ->
   *[other] { $count } รายงาน
}

# --- รายงานจากผู้ใช้ (ความคิดเห็นจากผู้ใช้) ---
# ใช้ events-untitled และ events-pagination-* (ร่วม ไฟล์เดียวกัน)
user-reports-title = รายงานจากผู้ใช้
user-reports-heading = รายงานจากผู้ใช้
user-reports-empty = ไม่พบรายงานจากผู้ใช้สำหรับโปรเจกต์นี้
user-reports-delete = ลบ
user-reports-delete-selected-confirm = ลบรายงานที่เลือกหรือไม่
user-reports-th-event-id = รหัสเหตุการณ์
user-reports-th-title = หัวข้อ
user-reports-th-timestamp = ประทับเวลา
user-reports-th-platform = แพลตฟอร์ม
user-reports-th-release = รีลีส
user-reports-select = เลือกรายงาน
user-reports-delete-all = ลบทั้งหมด { $count } รายการ
user-reports-delete-all-confirm = { $count ->
   *[other] ลบรายงานที่ตรงกันทั้งหมด { $count } รายการหรือไม่
}
user-reports-count = { $count ->
   *[other] { $count } รายงาน
}
