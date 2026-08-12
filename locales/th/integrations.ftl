# ส่วนตั้งค่าการเชื่อมต่อ: รายการ (templates/integrations.html) และแบบฟอร์ม
# "เพิ่ม" สามแบบ (webhook, slack, email) ใช้ nav-settings/nav-integrations สำหรับ
# ส่วนโครง integrations-empty มีมาร์กอัป <strong> แบบอินไลน์และเรนเดอร์ด้วย |safe
integrations-page-title = การเชื่อมต่อ — Stackpit
integrations-subtitle = ช่องทางส่งออกแบบ Webhook, Slack และอีเมล การกำหนดเส้นทางต่อโปรเจกต์ตั้งค่าได้ในการตั้งค่าของแต่ละโปรเจกต์
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + อีเมล
integrations-license-required-badge = ต้องมีใบอนุญาต
integrations-empty = ยังไม่มีการเชื่อมต่อ เพิ่มด้านบนเพื่อเริ่มรับการแจ้งเตือน หลังจากเพิ่มแล้ว ให้เปิดใช้งานต่อโปรเจกต์ที่ <strong>การตั้งค่าโปรเจกต์ → การเชื่อมต่อ</strong>
integrations-col-name = ชื่อ
integrations-col-type = ประเภท
integrations-col-endpoint = ปลายทาง
integrations-col-created = สร้างเมื่อ
integrations-delete-confirm = ลบการเชื่อมต่อนี้หรือไม่ ระบบจะนำออกจากทุกโปรเจกต์
integrations-test = ทดสอบ
integrations-delete = ลบ
flash-test-failed = ทดสอบไม่สำเร็จ: { $error }

# ป้ายกำกับ/ปุ่มร่วมของแบบฟอร์มเพิ่มการเชื่อมต่อทั้งสาม
integrations-cancel = ยกเลิก
integrations-optional = (ไม่บังคับ)
integrations-required = (บังคับ)
integrations-create = สร้างการเชื่อมต่อ

# --- เพิ่ม webhook ---
integrations-webhook-title = เพิ่ม webhook — Stackpit
integrations-webhook-breadcrumb = เพิ่ม webhook
integrations-webhook-heading = เพิ่มการเชื่อมต่อ webhook
integrations-webhook-name-placeholder = เช่น การแจ้งเตือนโปรดักชัน
integrations-webhook-url-label = URL ของ Webhook
integrations-webhook-secret-label = HMAC secret
integrations-webhook-secret-placeholder = signing secret แบบไม่บังคับ

# --- เพิ่ม Slack ---
integrations-slack-title = เพิ่ม Slack — Stackpit
integrations-slack-breadcrumb = เพิ่ม Slack
integrations-slack-heading = เพิ่มการเชื่อมต่อ Slack
integrations-slack-name-placeholder = เช่น ช่อง #alerts
integrations-slack-url-label = URL ของ Slack webhook

# --- เพิ่มอีเมล ---
integrations-email-title = เพิ่มอีเมล — Stackpit
integrations-email-breadcrumb = เพิ่มอีเมล
integrations-email-heading = เพิ่มการเชื่อมต่ออีเมล
integrations-email-name-placeholder = เช่น การแจ้งเตือนอีเมลทีม
integrations-email-lock-pre = ผู้ให้บริการและผู้ส่งมาจาก
integrations-email-lock-post = ของเซิร์ฟเวอร์ การเชื่อมต่อนี้เพียงเลือกผู้รับเท่านั้น
integrations-email-provider-label = ผู้ให้บริการ
integrations-email-token-label = โทเคน API
integrations-email-token-placeholder-default = เว้นว่างเพื่อใช้ค่าเริ่มต้น
integrations-email-token-placeholder = โทเคน API ของผู้ให้บริการ
integrations-email-from-label = ที่อยู่ผู้ส่ง
integrations-email-fromname-label = ชื่อผู้ส่ง
integrations-email-smtp-hint = SMTP ใช้การเชื่อมต่อ [email] ของเซิร์ฟเวอร์ ไม่จำเป็นต้องมีโทเคนต่ออินทิเกรชัน
