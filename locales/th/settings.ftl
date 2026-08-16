# ส่วนการตั้งค่า: หน้าค่าเริ่มต้นของเบราว์เซอร์ (templates/browser_defaults.html,
# คีย์ defaults-*) และหน้าจัดเตรียมองค์กรแบบเดี่ยว (templates/provision.html,
# คีย์ provision-*) ใช้ nav-settings สำหรับส่วนโครง ค่าระดับ (fatal/error/...)
# คงไว้ในเทมเพลตตามเดิม เช่นเดียวกับส่วนปัญหา/เหตุการณ์ที่เก็บระดับล็อกเป็นภาษาอังกฤษ

# --- ค่าเริ่มต้นของเบราว์เซอร์ ---
defaults-page-title = ค่าเริ่มต้นของเบราว์เซอร์ — Stackpit
defaults-subtitle = ตั้งค่าตัวกรองเริ่มต้นสำหรับหน้ารายการ เก็บเป็นคุกกี้ของเบราว์เซอร์
defaults-none = ไม่มีค่าเริ่มต้น
defaults-status-label = สถานะเริ่มต้น (ปัญหา)
defaults-status-unresolved = ยังไม่แก้ไข
defaults-status-resolved = แก้ไขแล้ว
defaults-status-ignored = เพิกเฉย
defaults-level-label = ระดับเริ่มต้น
defaults-period-label = ช่วงเวลาเริ่มต้น
defaults-save = บันทึกค่าเริ่มต้น
defaults-clear-confirm = ล้างค่าเริ่มต้นของเบราว์เซอร์ทั้งหมดหรือไม่
defaults-clear = ล้างค่าเริ่มต้นทั้งหมด
flash-defaults-saved = บันทึกค่าเริ่มต้นแล้ว
flash-defaults-cleared = ล้างค่าเริ่มต้นแล้ว

# --- ภาษาที่ต้องการ ---
settings-language-heading = ภาษาที่ต้องการ
settings-language-subtitle = เลือกภาษาสำหรับส่วนต่อประสาน Stackpit บัญชีที่เข้าสู่ระบบจะคงค่านี้ไว้ข้ามอุปกรณ์
settings-language-label = ภาษา
settings-language-save = บันทึกภาษา

settings-aria-sections = ส่วนการตั้งค่า

# --- หน้าจัดเตรียมองค์กร (หน้าเดี่ยว) ---
provision-page-title = ตั้งค่าองค์กร — Stackpit
provision-heading = ตั้งค่าองค์กร
provision-subtitle-1 = องค์กรต่อไปนี้พร้อมใช้งานจากผู้ให้บริการยืนยันตัวตนของคุณ
provision-subtitle-2 = เลือกองค์กรที่คุณต้องการสร้างใน Stackpit
provision-create = สร้างที่เลือก
provision-skip = ข้าม

# คิวการส่ง
queue-page-title = คิวการส่ง — Stackpit
queue-subtitle = การแจ้งเตือนที่ส่งไม่สำเร็จ ระบบจะลองใหม่อัตโนมัติเป็นเวลา 24 ชั่วโมง จากนั้นจะรอคุณอยู่ที่นี่
queue-count-pending = รออยู่ { $count } รายการ
queue-count-failed = ล้มเหลว { $count } รายการ
queue-empty = ไม่มีอะไรในคิว ส่งการแจ้งเตือนครบทุกรายการแล้ว
queue-col-integration = การเชื่อมต่อ
queue-col-project = โปรเจกต์
queue-col-state = สถานะ
queue-col-attempts = ครั้งที่ลอง
queue-col-queued = เข้าคิวเมื่อ
queue-col-error = ข้อผิดพลาดล่าสุด
queue-state-pending = กำลังลองใหม่
queue-state-failed = ยอมแพ้แล้ว
queue-replay = ส่งซ้ำ
queue-cancel = ทิ้ง
queue-cancel-confirm = ทิ้งการแจ้งเตือนนี้โดยไม่ส่งหรือไม่
