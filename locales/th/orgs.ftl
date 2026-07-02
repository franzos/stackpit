# ส่วนองค์กร: รายการองค์กร (templates/orgs.html) หน้าสมาชิก/คำเชิญ
# (templates/org_members.html) และหน้ารับคำเชิญแบบเดี่ยว
# (templates/invite_accept.html, คีย์ invite-*) ใช้ nav-organizations และ
# common-action-save ช่องว่างคั่นอยู่ในเทมเพลต ประโยคเตือนการลบและป้าย
# "พิมพ์ <slug> เพื่อยืนยัน" ถูกแยกที่จุด {{ var }} ค่า enum (member/owner, สถานะ)
# คงไว้ในเทมเพลตตามเดิม
orgs-page-title = องค์กร - Stackpit
orgs-subtitle = องค์กรที่คุณเป็นสมาชิก สลับระหว่างองค์กรหรือสร้างใหม่ได้
orgs-empty = คุณยังไม่ได้เป็นสมาชิกขององค์กรใด
orgs-col-organization = องค์กร
orgs-col-kind = ประเภท
orgs-members-btn = สมาชิก
orgs-active = กำลังใช้งาน
orgs-switch = สลับ
orgs-create-heading = สร้างองค์กร
orgs-create-desc = คุณจะกลายเป็นเจ้าของ slug จะถูกสร้างจากชื่อเมื่อเว้นว่าง
orgs-name = ชื่อ
orgs-slug = Slug
orgs-optional = (ไม่บังคับ)
orgs-create-submit = สร้างองค์กร

# --- หน้าสมาชิก ---
orgs-members-title-suffix = สมาชิก - Stackpit
orgs-members-word = สมาชิก
orgs-organization-word = องค์กร
orgs-slug-heading = Slug
orgs-slug-desc = ใช้ระบุองค์กรนี้ใน URL ต้องไม่ซ้ำกัน
orgs-email = อีเมล
orgs-role = บทบาท
orgs-role-member = สมาชิก
orgs-role-owner = เจ้าของ
orgs-member-fallback = ผู้ใช้ #{ $id }
orgs-joined = เข้าร่วมเมื่อ
orgs-promote = เลื่อนตำแหน่ง
orgs-demote = ลดตำแหน่ง
orgs-remove = นำออก
orgs-invites-heading = คำเชิญ
orgs-created = สร้างเมื่อ
orgs-expires = หมดอายุ
orgs-status = สถานะ
orgs-revoke = เพิกถอน
orgs-create-invite-heading = สร้างคำเชิญ
orgs-create-invite-desc = สร้างลิงก์คำเชิญที่ใช้ได้ครั้งเดียว
orgs-expiry-label = อายุ (วินาที)
orgs-expiry-hint = (ไม่บังคับ ค่าเริ่มต้น 7 วัน)
orgs-create-invite-submit = สร้างคำเชิญ
orgs-forseti-note = สมาชิกภาพขององค์กรนี้ถูกจัดการจากภายนอก
orgs-personal-note = นี่คือองค์กรส่วนตัว ไม่สามารถกำหนดค่าสมาชิกภาพได้
orgs-danger-heading = พื้นที่อันตราย
orgs-delete-danger-pre = การลบจะนำออก
orgs-delete-danger-projects = โปรเจกต์,
orgs-delete-danger-members = สมาชิก,
orgs-delete-danger-rest = รวมถึงเหตุการณ์ ปัญหา คีย์ การแจ้งเตือน และการเชื่อมต่อทั้งหมด การกระทำนี้ไม่สามารถย้อนกลับได้
orgs-confirm-type-pre = พิมพ์
orgs-confirm-type-post = เพื่อยืนยัน
orgs-delete-confirm = ลบองค์กรนี้และข้อมูลทั้งหมด การกระทำนี้ไม่สามารถย้อนกลับได้
orgs-delete-submit = ลบองค์กร

# --- รับคำเชิญ (หน้าเดี่ยว) ---
invite-page-title = คำเชิญเข้าองค์กร - Stackpit
invite-heading = คำเชิญเข้าองค์กร
invite-back-projects = กลับไปที่โปรเจกต์
invite-intro-pre = คุณได้รับคำเชิญให้เข้าร่วม
invite-intro-as = ในฐานะ
invite-intro-post = .
invite-accept-btn = ยอมรับคำเชิญ
invite-decline = ปฏิเสธ
invite-error-accepted = คำเชิญนี้ถูกยอมรับไปแล้ว
invite-error-expired = คำเชิญนี้หมดอายุแล้ว

# ข้อความตรวจสอบ/ข้อผิดพลาดที่เรนเดอร์บนหน้า html_error แปลที่จุดเรียกที่มี
# โลแคลของคำขอ ข้อผิดพลาดภายใน 5xx ยังคงเป็นภาษาอังกฤษ
orgs-err-name-required = ต้องระบุชื่อองค์กร
orgs-err-slug-taken = slug นี้ถูกใช้ไปแล้ว
orgs-err-invite-not-found = ไม่พบคำเชิญหรือคำเชิญไม่ถูกต้อง
orgs-err-org-not-found = ไม่พบองค์กร
orgs-err-last-owner-remove = ไม่สามารถนำเจ้าของคนสุดท้ายออกได้
orgs-err-last-owner-demote = ไม่สามารถลดตำแหน่งเจ้าของคนสุดท้ายได้
orgs-err-confirm-slug = พิมพ์ slug ขององค์กรเพื่อยืนยันการลบ
orgs-err-not-deletable = ไม่สามารถลบองค์กรนี้ได้
orgs-err-limit-reached = { $count ->
   *[other] คุณถึงขีดจำกัดที่ { $count } องค์กรแล้ว
}
