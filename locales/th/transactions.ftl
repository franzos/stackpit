# ส่วนทรานแซกชัน: รายการทรานแซกชันต่อโปรเจกต์และหน้ารายละเอียด (อินสแตนซ์)
# ใช้ nav-transactions สำหรับหัวข้อ/เบรดครัมบ์/ชื่อ ข้อความที่นับจำนวนใช้พหูพจน์ tv_count

# --- ส่วนต่อท้ายชื่อหน้า (ชื่อที่มีคำนำหน้าแบบไดนามิก) ---
transactions-title-suffix = — Stackpit

# --- รายการทรานแซกชัน ---
transactions-time-range = ช่วงเวลา
transactions-filter-submit = กรอง
transactions-list-empty = ไม่มีทรานแซกชันในช่วงเวลานี้
transactions-col-name = ทรานแซกชัน
transactions-col-throughput = ปริมาณงาน
transactions-col-failure = % ล้มเหลว
transactions-col-count = จำนวน
transactions-col-users = ผู้ใช้

# --- รายละเอียดทรานแซกชัน (อินสแตนซ์) ---
transactions-detail-op = op:
transactions-detail-empty = ไม่มีอินสแตนซ์ที่บันทึกสำหรับทรานแซกชันนี้
transactions-detail-col-duration = ระยะเวลา
transactions-detail-col-status = สถานะ
transactions-detail-col-trace = เทรซ
transactions-detail-col-when = เมื่อ
transactions-detail-distribution = การกระจายระยะเวลา
transactions-detail-spans = รายละเอียดตามสแปน
transactions-detail-issues = ปัญหาที่เกี่ยวข้อง
transactions-detail-instances = อินสแตนซ์ที่ช้าที่สุด
transactions-detail-trend = แนวโน้มเปอร์เซ็นไทล์
transactions-detail-trend-note = จุดที่ทำเครื่องหมายคือจุดที่ p95 สูงกว่าค่ามัธยฐานของห้าจุดก่อนหน้าเกิน 1.5 เท่า

# --- การแบ่งหน้า (รายละเอียดทรานแซกชัน) ---
transactions-pagination-label = การแบ่งหน้า
transactions-pagination-prev = « ก่อนหน้า
transactions-pagination-next = ถัดไป »
transactions-detail-count = { $count ->
   *[other] { $count } อินสแตนซ์
}
transactions-detail-failure-label = ล้มเหลว
