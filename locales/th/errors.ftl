# หน้าข้อผิดพลาดแบบเดี่ยว (src/html/mod.rs html_error) และหน้ายืนยันการสร้าง
# คำเชิญ (src/html/orgs.rs) ทั้งสองเรนเดอร์โดยไม่มีบริบทของคำขอ ข้อความจึงถูก
# แปลงที่โลแคลเริ่มต้น (อังกฤษ) แบรนด์ "Stackpit" คงไว้ตามเดิม
error-page-title = ข้อผิดพลาด - Stackpit
error-heading = ข้อผิดพลาด
error-not-found = ไม่พบหน้าที่คุณร้องขอ
error-back-projects = กลับไปที่โปรเจกต์

# หน้ายืนยันการสร้างคำเชิญ (อังกฤษเท่านั้น ไม่มีบริบทของคำขอ)
invite-created-page-title = สร้างคำเชิญแล้ว - Stackpit
invite-created-heading = สร้างคำเชิญแล้ว
invite-created-share = แชร์ลิงก์นี้ ใช้ได้ภายใน { $ttl } และใช้ได้ครั้งเดียว
invite-created-back-members = กลับไปที่สมาชิก

# --- ข้อความแฟลช / สำเร็จ / ตรวจสอบความถูกต้อง (รับรู้โลแคล) ---
# ส่งออกโดยตัวจัดการเว็บเป็นข้อความแบนเนอร์ครั้งเดียว คำนำหน้า "Error: {e}"
# ถูกเติมใน Rust ผ่าน common-error-prefix

# การวินิจฉัยไม่พบ คำนำหน้า "Error:"/"ข้อผิดพลาด:" ถูกเติมใน Rust ค่าตรงนี้มี
# เพียงวลีของเอนทิตีพร้อม id
flash-not-found-project = ไม่พบโปรเจกต์: { $id }
flash-not-found-key = ไม่พบคีย์ API: { $id }
flash-not-found-integration = ไม่พบการเชื่อมต่อ: { $id }
flash-not-found-alert-rule = ไม่พบกฎการแจ้งเตือน: { $id }
flash-not-found-digest-schedule = ไม่พบกำหนดการสรุป: { $id }
flash-not-found-repo = ไม่พบที่เก็บโค้ด: { $id }
flash-not-found-project-integration = ไม่พบการเชื่อมต่อของโปรเจกต์: { $id }
flash-not-found-filter = ไม่พบ { $label }

# การตรวจสอบกฎตัวกรอง
flash-unrecognized-field = ฟิลด์ที่ไม่รู้จัก: { $value }
flash-unrecognized-operator = ตัวดำเนินการที่ไม่รู้จัก: { $value }
flash-unrecognized-action = การกระทำที่ไม่รู้จัก: { $value }

# การตั้งค่าโปรเจกต์
flash-project-name-updated = อัปเดตชื่อโปรเจกต์แล้ว
flash-project-name-too-long = ชื่อโปรเจกต์เกินความยาวสูงสุด { $max } อักขระ
flash-repo-url-required = ต้องระบุ URL ของที่เก็บโค้ด
flash-repo-url-too-long = URL ของที่เก็บโค้ดเกินความยาวสูงสุด 2048 อักขระ
flash-repo-added = เพิ่มที่เก็บโค้ดแล้ว
flash-repo-removed = นำที่เก็บโค้ดออกแล้ว
flash-project-archived = จัดเก็บโปรเจกต์แล้ว
flash-project-unarchived = ยกเลิกการจัดเก็บโปรเจกต์แล้ว
flash-key-created = สร้างคีย์แล้ว
flash-key-deleted = ลบคีย์แล้ว

# การแจ้งเตือนและสรุป
flash-project-not-found-or-denied = ข้อผิดพลาด: ไม่พบโปรเจกต์หรือถูกปฏิเสธการเข้าถึง
flash-alert-rule-created = สร้างกฎการแจ้งเตือนแล้ว
flash-alert-rule-deleted = ลบกฎการแจ้งเตือนแล้ว
flash-digest-schedule-created = สร้างกำหนดการสรุปแล้ว
flash-digest-schedule-deleted = ลบกำหนดการสรุปแล้ว

# การเชื่อมต่อของโปรเจกต์
flash-integration-not-found = ไม่พบการเชื่อมต่อ
flash-integration-activated = เปิดใช้งานการเชื่อมต่อแล้ว
flash-integration-updated = อัปเดตการเชื่อมต่อแล้ว
flash-integration-deactivated = ปิดใช้งานการเชื่อมต่อแล้ว

# การเชื่อมต่อขององค์กร
flash-name-required = ต้องระบุชื่อ
flash-invalid-integration-kind = ประเภทการเชื่อมต่อไม่ถูกต้อง
flash-invalid-email-provider = ผู้ให้บริการอีเมลไม่ถูกต้อง
flash-api-token-required = ต้องระบุโทเคน API
flash-from-address-required = ต้องระบุที่อยู่ผู้ส่ง
flash-smtp-not-configured = ยังไม่ได้ตั้งค่า SMTP โปรดกำหนด [email] host ในการตั้งค่าเซิร์ฟเวอร์
flash-invalid-to-address = ผู้รับต้องเป็นที่อยู่อีเมลที่ถูกต้อง
flash-test-digest-sent = จัดคิวไดเจสต์ทดสอบสำหรับ { $count } โปรเจกต์ ไปยังการเชื่อมต่อที่เปิดใช้งานไดเจสต์
flash-test-digest-sample = ไม่มีกิจกรรมล่าสุด จึงจัดคิวไดเจสต์ตัวอย่างที่มีป้ายกำกับไว้
flash-test-digest-no-target = ไม่มีการเชื่อมต่อใดที่เปิดใช้งานไดเจสต์สำหรับโปรเจกต์ของกำหนดการนี้
flash-url-required = ต้องระบุ URL
flash-secret-not-configured = ไม่สามารถเก็บ secret ได้: ยังไม่ได้ตั้งค่าการเข้ารหัส ตั้งค่า STACKPIT_MASTER_KEY เพื่อเปิดใช้งานการเก็บ secret
flash-integration-license-required = การเชื่อมต่อ Slack เว็บฮุก และตัวติดตามปัญหา ต้องใช้ใบอนุญาตเชิงพาณิชย์ที่ยังใช้งานได้ การแจ้งเตือนทางอีเมลยังใช้งานได้โดยไม่ต้องมีใบอนุญาต
flash-integration-created = สร้างการเชื่อมต่อแล้ว
flash-integration-name-exists = มีการเชื่อมต่อที่ใช้ชื่อนี้อยู่แล้ว
flash-integration-deleted = ลบการเชื่อมต่อแล้ว
flash-integration-no-url = การเชื่อมต่อยังไม่ได้ตั้งค่า URL
flash-test-notification-sent = ส่งการแจ้งเตือนทดสอบแล้ว

# ตัวกรองขาเข้า
flash-inbound-filters-updated = อัปเดตตัวกรองขาเข้าแล้ว
flash-pattern-required = ต้องระบุรูปแบบ
flash-message-filter-added = เพิ่มตัวกรองข้อความแล้ว
flash-message-filter-removed = นำตัวกรองข้อความออกแล้ว
flash-rate-limit-updated = อัปเดตการจำกัดอัตราแล้ว
flash-environment-required = ต้องระบุสภาพแวดล้อม
flash-environment-excluded = ยกเว้นสภาพแวดล้อมแล้ว
flash-environment-filter-removed = นำตัวกรองสภาพแวดล้อมออกแล้ว
flash-release-filter-added = เพิ่มตัวกรองรีลีสแล้ว
flash-release-filter-removed = นำตัวกรองรีลีสออกแล้ว
flash-ua-filter-added = เพิ่มตัวกรอง user-agent แล้ว
flash-ua-filter-removed = นำตัวกรอง user-agent ออกแล้ว
flash-rule-added = เพิ่มกฎแล้ว
flash-rule-removed = นำกฎออกแล้ว
flash-cidr-required = ต้องระบุ CIDR
flash-invalid-cidr = รูปแบบ CIDR ไม่ถูกต้อง
flash-ip-block-added = เพิ่มบล็อก IP แล้ว
flash-ip-block-removed = นำบล็อก IP ออกแล้ว

# โปรเจกต์ใหม่
flash-project-name-required = ต้องระบุชื่อโปรเจกต์
flash-email-not-configured = ยังไม่ได้ตั้งค่าอีเมล เพิ่มส่วน [email] พร้อมผู้ให้บริการลงในไฟล์ตั้งค่าเซิร์ฟเวอร์
flash-integration-saved = อัปเดตการเชื่อมต่อแล้ว
flash-integration-global-not-for-trackers = ตัวติดตามปัญหาไม่ใช้การส่งทั่วทั้งองค์กร ปลายทางที่จะสร้างปัญหามาจากการตั้งค่าที่เก็บโค้ดของแต่ละโปรเจกต์
flash-project-excluded = ยกเว้นโปรเจกต์นี้จากการเชื่อมต่อนี้แล้ว
flash-project-included = ไม่ได้ยกเว้นโปรเจกต์นี้อีกต่อไป
flash-global-email-needs-recipient = การเชื่อมต่ออีเมลทั่วทั้งองค์กรต้องมีผู้รับเริ่มต้น เพราะโปรเจกต์ที่ไม่เคยเปิดใช้จะไม่มีที่อยู่ของตัวเอง
flash-queue-item-not-found = ไม่พบการแจ้งเตือนในคิว
flash-queue-replayed = ส่งการแจ้งเตือนสำเร็จและนำออกจากคิวแล้ว
flash-queue-replay-failed = ส่งซ้ำไม่สำเร็จ: { $error }
flash-queue-cancelled = ทิ้งการแจ้งเตือนในคิวแล้ว
flash-queue-replay-failed-generic = ส่งซ้ำไม่สำเร็จ ดูสาเหตุได้ที่รายการในคิว ใต้หัวข้อข้อผิดพลาด
flash-license-activated = เปิดใช้งานสัญญาอนุญาตแล้ว
flash-org-cap-reached = ถึงขีดจำกัดจำนวนองค์กรของสัญญาอนุญาตแล้ว องค์กรบางแห่งจึงไม่ถูกสร้าง
flash-license-deactivated = นำสัญญาอนุญาตออกแล้ว
flash-license-persist-failed = ตรวจสอบสัญญาอนุญาตผ่านแล้วแต่บันทึกไม่สำเร็จ ดูบันทึกของเซิร์ฟเวอร์
flash-license-clear-failed = นำสัญญาอนุญาตออกไม่สำเร็จ ดูบันทึกของเซิร์ฟเวอร์
flash-license-empty = วางคีย์สัญญาอนุญาตเพื่อเปิดใช้งาน
flash-license-bad-signature = สัญญาอนุญาตนี้ใช้กับการติดตั้งนี้ไม่ได้ ตรวจสอบว่าวางคีย์ถูกต้อง
flash-license-wrong-product = สัญญาอนุญาตนี้ไม่ใช่ของ Stackpit
flash-license-unreadable = อ่านสัญญาอนุญาตนี้ไม่ได้ ตรวจสอบแล้วลองใหม่
