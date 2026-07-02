# หน้าเข้าสู่ระบบแบบเดี่ยว (templates/login.html) พร้อมข้อความแบนเนอร์ OAuth/
# ออกจากระบบ ที่สร้างใน src/html/login.rs login-token-help มีมาร์กอัป <code>
# แบบอินไลน์และเรนเดอร์ด้วย |safe
login-page-title = เข้าสู่ระบบ — Stackpit
login-welcome = ยินดีต้อนรับกลับมา
login-subtitle = เข้าสู่ระบบเพื่อจัดการการติดตามข้อผิดพลาดของคุณ
login-sso = เข้าสู่ระบบด้วย SSO
login-or = หรือ
login-token-label = Admin Token
login-token-placeholder = กรอก master token ของคุณ…
login-submit = เข้าสู่ระบบ
login-token-help = Admin token มาจาก <code class="text-mono">admin_token</code> ใน <code class="text-mono">stackpit.toml</code> แก้ไขไฟล์แล้วรีสตาร์ท <code class="text-mono">stackpit serve</code> เพื่อให้การเปลี่ยนแปลงมีผล
login-docs = เอกสารประกอบ
login-selfhosting = คู่มือการโฮสต์ด้วยตนเอง

# แบนเนอร์ข้อผิดพลาด (แปลงจากรหัส ?error= ของ OAuth) และแบนเนอร์แจ้งการออกจากระบบ
login-error-state-mismatch = เซสชันการเข้าสู่ระบบของคุณถูกดัดแปลงหรือหมดอายุแล้ว กรุณาลองอีกครั้ง
login-error-session-expired = เซสชันของคุณหมดอายุแล้ว กรุณาเข้าสู่ระบบอีกครั้ง
login-error-missing-response = ผู้ให้บริการยืนยันตัวตนของคุณส่งคำตอบที่ไม่สมบูรณ์กลับมา กรุณาลองอีกครั้ง
login-error-token-exchange = เราไม่สามารถเข้าสู่ระบบกับผู้ให้บริการยืนยันตัวตนของคุณได้ กรุณาลองอีกครั้งในอีกสักครู่
login-error-provisioning = ไม่สามารถสร้างบัญชีของคุณได้ กรุณาติดต่อผู้ดูแลระบบ
login-error-email-conflict = มีบัญชีที่ใช้อีเมลนี้อยู่แล้ว กรุณาติดต่อผู้ดูแลระบบ
login-error-session-unavailable = การเข้าสู่ระบบไม่พร้อมใช้งานชั่วคราว กรุณาลองอีกครั้งในอีกสักครู่
login-error-encryption = การเข้าสู่ระบบตั้งค่าไม่ถูกต้องบนดีพลอยเมนต์นี้ กรุณาติดต่อผู้ดูแลระบบ
login-error-generic = เข้าสู่ระบบไม่สำเร็จ กรุณาลองอีกครั้ง
login-error-invalid-token = โทเคนไม่ถูกต้อง
login-logout-local = ออกจากระบบ Stackpit แล้ว เซสชันกับผู้ให้บริการยืนยันตัวตนของคุณยังไม่ถูกปิด -- หากจำเป็นให้ออกจากระบบที่นั่นแยกต่างหาก
