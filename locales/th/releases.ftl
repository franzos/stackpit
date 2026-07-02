# ส่วนรีลีส: รายการรีลีสข้ามโปรเจกต์และหน้าสุขภาพรีลีสต่อโปรเจกต์
# ใช้ nav-releases และ nav-health ข้อความที่นับจำนวนใช้พหูพจน์ tv_count

# --- ส่วนต่อท้ายชื่อหน้า ---
releases-title-suffix = — Stackpit

# --- รายการรีลีส ---
releases-list-search-placeholder = ค้นหารีลีส…
releases-list-search-label = ค้นหารีลีส
releases-list-project-placeholder = รหัสโปรเจกต์
releases-list-project-label = กรองตามโปรเจกต์
releases-list-period-label = ช่วงการนำไปใช้
releases-list-period-24h = 24 ชม. ที่ผ่านมา
releases-list-period-7d = 7 วันที่ผ่านมา
releases-list-period-30d = 30 วันที่ผ่านมา
releases-filter-submit = กรอง
releases-list-empty = ยังไม่มีรีลีส ตั้งค่า <code class="text-mono">release</code> บน SDK ของคุณ แล้วรีลีสจะปรากฏที่นี่เมื่อมีเหตุการณ์เข้ามา
releases-col-version = เวอร์ชัน
releases-col-project = โปรเจกต์
releases-col-issues = ปัญหา
releases-col-events = เหตุการณ์
releases-col-adoption = การนำไปใช้
releases-col-first-seen = พบครั้งแรก
releases-col-last-seen = พบล่าสุด

# --- การแบ่งหน้า ---
releases-pagination-label = การแบ่งหน้า
releases-pagination-prev = « ก่อนหน้า
releases-pagination-next = ถัดไป »
releases-count = { $count ->
   *[other] { $count } รีลีส
}

# --- สุขภาพรีลีส ---
release-health-title = สุขภาพรีลีส
release-health-heading = สุขภาพรีลีส
release-health-sessions-heading = เซสชันตามช่วงเวลา
release-health-empty = ไม่มีข้อมูลเซสชัน เหตุการณ์เซสชันที่มีฟิลด์ <code class="text-mono">status</code> จะปรากฏที่นี่
release-health-col-release = รีลีส
release-health-col-sessions = เซสชัน
release-health-col-ok = OK
release-health-col-crashed = ล่ม
release-health-col-errored = มีข้อผิดพลาด
release-health-col-crash-free-sessions = เซสชันที่ไม่ล่ม
release-health-col-crash-free-users = ผู้ใช้ที่ไม่พบการล่ม
release-health-na = ไม่มีข้อมูล
