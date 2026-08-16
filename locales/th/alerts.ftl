# หน้าตั้งค่าการแจ้งเตือน &amp; สรุป (templates/alerts.html) ใช้ nav-settings
# และ nav-alerts-digests สำหรับส่วนโครง ช่องว่างคั่นอยู่ในเทมเพลต ค่าจึงไม่มี
# ช่องว่างนำหน้า/ตามหลัง alerts-page-title คง &amp; ไว้ตามเดิมและเรนเดอร์ด้วย |safe
alerts-page-title = การแจ้งเตือน &amp; สรุป — Stackpit
alerts-notify-help-pre = การแจ้งเตือนจะถูกส่งผ่านการเชื่อมต่อในหน้า
alerts-notify-help-post = ดังกล่าว

# --- ประเภทการแจ้งเตือน ---
alerts-notify-types-heading = ประเภทการแจ้งเตือน
alerts-notify-types-desc = การแจ้งเตือนปัญหาใหม่และการถดถอยจะทำงานทุกครั้งที่พบปัญหาใหม่หรือปัญหากลับมาอีกครั้ง กฎค่าขีดเริ่มทำงานตามปริมาณเหตุการณ์ในช่วงเวลาหนึ่ง ส่วนสรุปเป็นภาพรวมตามรอบเวลา รายการนี้แสดงเฉพาะการเชื่อมต่อที่โปรเจกต์เชื่อมเอง ส่วนการเชื่อมต่อระดับองค์กรจะส่งไปยังทุกโปรเจกต์และจัดการได้จากหน้าการเชื่อมต่อ
alerts-notify-types-empty = ยังไม่มีโปรเจกต์ใดเชื่อมต่อของตัวเอง การเชื่อมต่อระดับองค์กรจะไม่แสดงที่นี่และอาจกำลังส่งอยู่ ดูได้จากหน้าการเชื่อมต่อ
alerts-col-integration = การเชื่อมต่อ
alerts-col-new-issues = ปัญหาใหม่
alerts-col-regressions = การถดถอย
alerts-col-digests = สรุป
alerts-notify-save = บันทึก

# --- กฎเกณฑ์ค่าขีดเริ่ม ---
alerts-threshold-heading = กฎค่าขีดเริ่ม
alerts-threshold-desc = แจ้งเตือนเมื่อปัญหาได้รับเหตุการณ์มากกว่า N ครั้งในช่วงเวลาที่กำหนด
alerts-rules-empty = ยังไม่มีกฎการแจ้งเตือน
alerts-col-scope = ขอบเขต
alerts-col-issue = ปัญหา
alerts-col-threshold = ค่าขีดเริ่ม
alerts-col-window = ช่วงเวลา
alerts-col-cooldown = เวลาพัก
alerts-scope-global = ทั่วทั้งระบบ
alerts-fingerprint-any = ใดก็ได้
alerts-rule-delete-confirm = ลบกฎการแจ้งเตือนนี้หรือไม่
alerts-delete-label = ลบ
alerts-add-rule = + เพิ่มกฎการแจ้งเตือน
alerts-all-projects = ทุกโปรเจกต์
alerts-project-fallback = โปรเจกต์ { $id }
alerts-fingerprint-label = ลายนิ้วมือของปัญหา
alerts-fingerprint-hint = (เว้นว่าง = ใดก็ได้)
alerts-fingerprint-placeholder = ปัญหาใดก็ได้
alerts-fingerprint-help = ลายนิ้วมือระบุปัญหาหนึ่งรายการ (เหตุการณ์ที่จัดกลุ่มไว้) มองเห็นได้ใน URL ของหน้าปัญหาใดก็ได้ เว้นว่างเพื่อครอบคลุมทุกปัญหาในขอบเขต
alerts-unit-s = (วินาที)
alerts-create-rule = สร้างกฎ

# --- กำหนดการสรุป ---
alerts-digest-heading = กำหนดการสรุป
alerts-digest-desc = สรุปกิจกรรมเป็นระยะ — รายวันหรือรายสัปดาห์ แทนการแจ้งรบกวนทีละเหตุการณ์
alerts-digests-empty = ยังไม่มีกำหนดการสรุป
alerts-col-interval = ช่วงเวลา
alerts-col-last-sent = ส่งล่าสุด
alerts-col-enabled = เปิดใช้งาน
alerts-never = ไม่เคย
alerts-yes = ใช่
alerts-no = ไม่
alerts-digest-delete-confirm = ลบกำหนดการสรุปนี้หรือไม่
alerts-add-digest = + เพิ่มกำหนดการสรุป
alerts-interval-daily = รายวัน (24 ชม.)
alerts-interval-weekly = รายสัปดาห์ (7 วัน)
alerts-interval-hourly = รายชั่วโมง
alerts-create-schedule = สร้างกำหนดการ
