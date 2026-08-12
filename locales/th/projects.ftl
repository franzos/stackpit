# ส่วนโปรเจกต์: รายการ, สร้างใหม่, การตั้งค่า (ทั่วไป/คีย์/sourcemaps/ตัวกรอง),
# การเชื่อมต่อ และหน้ายืนยันการสร้าง ค่าที่เรนเดอร์ด้วย |safe มีมาร์กอัป HTML
# แบบอินไลน์ แท็กคงไว้ตามเดิม แปลเฉพาะข้อความ

# --- รายการโปรเจกต์ ---
projects-list-title = โปรเจกต์ — Stackpit
projects-list-heading = โปรเจกต์
projects-list-subtitle = ติดตามสุขภาพทั่วทั้งสถาปัตยกรรมของคุณ
projects-list-all-events = เหตุการณ์ทั้งหมด
projects-list-all-releases = รีลีสทั้งหมด
projects-list-new = + โปรเจกต์ใหม่
projects-list-search-placeholder = ค้นหาโปรเจกต์ตามชื่อ แพลตฟอร์ม หรือเจ้าของ…
projects-list-search-label = ค้นหาโปรเจกต์
projects-list-filter = กรอง
projects-org-filter-label = กรองตามองค์กร
projects-org-filter-all = ทุกองค์กร
projects-list-empty = ไม่พบโปรเจกต์ เหตุการณ์จะปรากฏที่นี่เมื่อมีการรับข้อมูล
projects-period-label = ช่วงเวลา
projects-col-project = โปรเจกต์
projects-col-platforms = แพลตฟอร์ม
projects-col-issues = ปัญหา
projects-col-events = เหตุการณ์
projects-col-breakdown = รายละเอียดแยกส่วน
projects-col-release = รีลีส
projects-col-first-seen = พบครั้งแรก
projects-col-last-seen = พบล่าสุด
projects-breakdown-errors = ข้อผิดพลาด:
projects-breakdown-transactions = ทรานแซกชัน:
projects-breakdown-sessions = เซสชัน:
projects-breakdown-other = อื่น ๆ:
projects-legend-errors = ข้อผิดพลาด
projects-legend-transactions = ทรานแซกชัน
projects-legend-sessions = เซสชัน
projects-legend-other = อื่น ๆ

# --- ใช้ร่วมในแบบฟอร์มโปรเจกต์ ---
projects-optional = (ไม่บังคับ)
projects-cancel = ยกเลิก
projects-remove = นำออก
projects-delete = ลบ
projects-name-placeholder = โปรเจกต์ของฉัน

# --- โปรเจกต์ใหม่ ---
projects-new-title = โปรเจกต์ใหม่ — Stackpit
projects-new-heading = โปรเจกต์ใหม่
projects-new-name-label = ชื่อโปรเจกต์
projects-new-platform-label = แพลตฟอร์ม
projects-new-platform-select = เลือกแพลตฟอร์ม…
projects-new-platform-other = อื่น ๆ
projects-new-platform-native = Native (C/C++)
projects-new-submit = สร้างโปรเจกต์

# --- แท็บการตั้งค่า (ใช้ร่วมโดยหน้าตั้งค่า) ---
projects-tab-general = ทั่วไป
projects-tab-sdk = ตั้งค่า SDK
projects-tab-sourcemaps = Source maps
projects-tab-filters = ตัวกรอง
projects-tab-integrations = การเชื่อมต่อ

# --- การตั้งค่า: ทั่วไป ---
projects-settings-heading = การตั้งค่า
projects-settings-archived = (จัดเก็บแล้ว)
projects-settings-name-heading = ชื่อโปรเจกต์
projects-settings-display-name = ชื่อที่แสดง
projects-settings-save-name = บันทึกชื่อ
projects-settings-info-heading = ข้อมูลโปรเจกต์
projects-settings-status = สถานะ
projects-settings-source = แหล่งที่มา
projects-repos-heading = ที่เก็บซอร์สโค้ด
projects-repos-help = เชื่อม stack frame กับซอร์สโค้ดบน forge ของคุณ ลงทะเบียนรีลีสพร้อม commit SHA ผ่าน <code class="text-mono">sentry-cli</code> เพื่อเปิดใช้งานลิงก์
projects-repos-empty = ยังไม่ได้ตั้งค่าที่เก็บโค้ด
projects-repos-url-label = URL ของที่เก็บโค้ด
projects-repos-col-forge = Forge
projects-repos-template = เทมเพลต URL
projects-repos-auto = อัตโนมัติ
projects-repos-remove-confirm = นำที่เก็บโค้ดนี้ออกหรือไม่
projects-repos-add = เพิ่มที่เก็บโค้ด
projects-repos-add-help = เพิ่มลิงก์ซอร์สโค้ดที่คลิกได้ (เช่น "ดูบน GitHub") ข้าง stack frame ต้องมีรีลีสพร้อม commit SHA — ระบบตรวจจับประเภท forge อัตโนมัติ รองรับ: GitHub, GitLab, Gitea/Codeberg, Bitbucket, Sourcehut, Gitee, Azure DevOps สำหรับ forge อื่น ให้ระบุเทมเพลต URL
projects-danger-heading = พื้นที่อันตราย
projects-archive-desc = จัดเก็บโปรเจกต์นี้ โปรเจกต์ที่จัดเก็บแล้วจะปฏิเสธเหตุการณ์ใหม่
projects-archive-confirm = จัดเก็บโปรเจกต์นี้หรือไม่ เหตุการณ์ใหม่จะถูกปฏิเสธ
projects-archive-submit = จัดเก็บโปรเจกต์
projects-unarchive-desc = ยกเลิกการจัดเก็บโปรเจกต์นี้เพื่อกลับมารับเหตุการณ์
projects-unarchive-submit = ยกเลิกการจัดเก็บโปรเจกต์
projects-delete-desc = ลบโปรเจกต์นี้และข้อมูลทั้งหมดอย่างถาวร การกระทำนี้ไม่สามารถย้อนกลับได้
projects-delete-confirm = ลบโปรเจกต์นี้และข้อมูลทั้งหมดหรือไม่ การกระทำนี้ไม่สามารถย้อนกลับได้
projects-delete-submit = ลบโปรเจกต์
projects-move-heading = ย้ายไปยังองค์กรอื่น
projects-move-desc = ย้ายโปรเจกต์นี้ไปยังองค์กรอื่นที่คุณเป็นเจ้าของ ข้อมูลและ DSN ยังคงใช้งานได้ แต่การเชื่อมต่อการแจ้งเตือนจะถูกยกเลิกและต้องเพิ่มใหม่ในองค์กรใหม่
projects-move-target-label = องค์กรปลายทาง
projects-move-confirm-pre = พิมพ์
projects-move-confirm-post = เพื่อยืนยัน
projects-move-confirm-placeholder = ชื่อโปรเจกต์
projects-move-confirm-dialog = ย้ายโปรเจกต์นี้ไปยังองค์กรที่เลือกหรือไม่?
projects-move-submit = ย้ายโปรเจกต์
projects-move-err-invalid-target = องค์กรปลายทางไม่ถูกต้อง
projects-move-err-name-mismatch = ชื่อโปรเจกต์ไม่ตรงกัน
projects-move-err-denied = คุณไม่ใช่เจ้าของขององค์กรปลายทาง
projects-move-err-conflict = ไม่สามารถย้ายโปรเจกต์ได้ อาจมีการเปลี่ยนแปลง โปรดลองอีกครั้ง

# --- การตั้งค่า: ตั้งค่า SDK / คีย์ ---
projects-keys-title = ตั้งค่า SDK
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = ยังไม่ได้ลงทะเบียนคีย์ สร้างคีย์ด้านล่างเพื่อรับ DSN
projects-keys-list-heading = คีย์ของโปรเจกต์
projects-keys-empty = ยังไม่ได้ลงทะเบียนคีย์สำหรับโปรเจกต์นี้
projects-keys-col-public = คีย์สาธารณะ
projects-keys-col-label = ป้ายกำกับ
projects-keys-col-status = สถานะ
projects-keys-col-created = สร้างเมื่อ
projects-keys-delete-confirm = ลบคีย์นี้หรือไม่ SDK ที่ใช้คีย์นี้จะหยุดทำงาน
projects-keys-create-heading = สร้างคีย์
projects-keys-label-label = ป้ายกำกับ
projects-keys-label-placeholder = เช่น production, staging
projects-keys-create-submit = สร้างคีย์

# --- การตั้งค่า: source maps ---
projects-sourcemaps-title = Source Maps
projects-sourcemaps-apikey-heading = คีย์ API
projects-sourcemaps-apikey-desc = การอัปโหลด source map ต้องใช้คีย์ API เฉพาะโปรเจกต์นี้และใช้ได้กับการดำเนินการ source map เท่านั้น
projects-sourcemaps-key-generated = สร้างคีย์แล้ว:
projects-sourcemaps-key-warning = คัดลอกคีย์นี้ทันที — คีย์จะไม่ถูกแสดงอีก
projects-sourcemaps-col-key = คีย์
projects-sourcemaps-regen-confirm = สร้างคีย์ใหม่หรือไม่ คีย์ปัจจุบันจะหยุดทำงาน
projects-sourcemaps-regen = สร้างใหม่
projects-sourcemaps-empty = ไม่มีคีย์ API สำหรับ source map ของโปรเจกต์นี้
projects-sourcemaps-generate = สร้างคีย์
projects-sourcemaps-setup-heading = การตั้งค่า
projects-sourcemaps-setup-desc = ใช้ <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> เพื่ออัปโหลด source map ตั้งค่าตัวแปรสภาพแวดล้อมเหล่านี้:
projects-sourcemaps-then-upload = จากนั้นอัปโหลด:

# --- การตั้งค่า: ตัวกรอง ---
projects-filters-inbound-heading = ตัวกรองขาเข้า
projects-filters-inbound-desc = ตัวกรองในตัวที่ทิ้งเหตุการณ์ซึ่งตรงกับรูปแบบรบกวนทั่วไป
projects-filters-browser-ext = ส่วนขยายเบราว์เซอร์ — ทิ้งเหตุการณ์จากส่วนขยาย Chrome/Firefox/Safari
projects-filters-localhost = Localhost — ทิ้งเหตุการณ์จาก localhost, 127.0.0.1, IP ส่วนตัว
projects-filters-inbound-submit = บันทึกตัวกรองขาเข้า
projects-filters-message-heading = ตัวกรองข้อความ
projects-filters-message-help = รูปแบบ glob ที่จับคู่กับหัวข้อเหตุการณ์ ใช้ <code class="text-mono">*</code> แทนลำดับใด ๆ, <code class="text-mono">?</code> แทนอักขระเดียว
projects-filters-col-pattern = รูปแบบ
projects-filters-message-empty = ยังไม่ได้ตั้งค่าตัวกรองข้อความ
projects-filters-add-pattern = เพิ่มรูปแบบ
projects-filters-message-submit = เพิ่มตัวกรองข้อความ
projects-filters-ratelimit-heading = การจำกัดอัตรา
projects-filters-ratelimit-desc = จำนวนเหตุการณ์สูงสุดต่อนาทีสำหรับโปรเจกต์นี้ 0 = ไม่จำกัด
projects-filters-ratelimit-label = เหตุการณ์ต่อนาที
projects-filters-ratelimit-submit = บันทึกการจำกัดอัตรา
projects-filters-env-heading = สภาพแวดล้อมที่ยกเว้น
projects-filters-env-desc = เหตุการณ์จากสภาพแวดล้อมเหล่านี้จะถูกทิ้งอย่างเงียบ ๆ
projects-filters-col-environment = สภาพแวดล้อม
projects-filters-env-empty = ไม่มีสภาพแวดล้อมที่ยกเว้น
projects-filters-env-add-label = เพิ่มสภาพแวดล้อมที่ยกเว้น
projects-filters-env-submit = ยกเว้นสภาพแวดล้อม
projects-filters-release-heading = ตัวกรองรีลีส
projects-filters-release-desc = รูปแบบ glob ที่จับคู่กับเวอร์ชันรีลีส เหตุการณ์ที่ตรงกันจะถูกทิ้ง
projects-filters-release-empty = ไม่มีตัวกรองรีลีส
projects-filters-release-submit = เพิ่มตัวกรองรีลีส
projects-filters-ua-heading = ตัวกรอง user-agent
projects-filters-ua-desc = รูปแบบ glob ที่จับคู่กับเฮดเดอร์ User-Agent รูปแบบในตัวสำหรับ kube-probe และตัวตรวจสอบสุขภาพจะทำงานเสมอ
projects-filters-ua-empty = ไม่มีตัวกรอง user-agent แบบกำหนดเอง
projects-filters-ua-submit = เพิ่มตัวกรอง user-agent
projects-filters-rules-heading = กฎแบบกำหนดเอง
projects-filters-rules-desc = กฎขั้นสูงที่จับคู่กับฟิลด์ของเหตุการณ์ กฎที่มีลำดับความสำคัญสูงกว่าจะถูกประเมินก่อน
projects-filters-col-field = ฟิลด์
projects-filters-col-operator = ตัวดำเนินการ
projects-filters-col-value = ค่า
projects-filters-col-action = การกระทำ
projects-filters-col-priority = ลำดับความสำคัญ
projects-filters-rules-empty = ไม่มีกฎแบบกำหนดเอง
projects-filters-sample-rate-label = อัตราการสุ่มตัวอย่าง
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = เพิ่มกฎ
projects-filters-op = { $op ->
    [not_equals] ไม่เท่ากับ
    [contains] มี
    [not_contains] ไม่มี
    [starts_with] ขึ้นต้นด้วย
    [in] อยู่ใน
    [not_in] ไม่อยู่ใน
   *[equals] เท่ากับ
}
projects-filters-action = { $action ->
    [sample] สุ่มตัวอย่าง
   *[drop] ทิ้ง
}
projects-filters-ip-heading = รายการบล็อก IP
projects-filters-ip-desc = บล็อก CIDR หรือ IP รายตัว เหตุการณ์จาก IP ที่ถูกบล็อกจะถูกทิ้งอย่างเงียบ ๆ
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = ยังไม่ได้ตั้งค่าบล็อก IP
projects-filters-ip-add-label = เพิ่ม CIDR
projects-filters-ip-submit = บล็อกช่วง IP
projects-filters-discard-heading = สถิติการทิ้ง
projects-filters-discard-window = (7 วันที่ผ่านมา)
projects-filters-col-date = วันที่
projects-filters-col-reason = เหตุผล
projects-filters-col-count = จำนวน

# ป้ายกำกับเอนทิตีของตัวกรอง แทรกใน flash-not-found-filter ตอนลบ
projects-filter-label-message = ตัวกรองข้อความ
projects-filter-label-environment = ตัวกรองสภาพแวดล้อม
projects-filter-label-release = ตัวกรองรีลีส
projects-filter-label-user-agent = ตัวกรอง user-agent
projects-filter-label-rule = กฎตัวกรอง

# --- การตั้งค่า: การเชื่อมต่อ ---
projects-integrations-active-heading = การเชื่อมต่อที่ใช้งาน
projects-integrations-active-empty = ยังไม่ได้เปิดใช้งานการเชื่อมต่อ เพิ่มการเชื่อมต่อระดับทั่วทั้งระบบที่หน้า <a class="text-primary" href="/web/settings/integrations/">การเชื่อมต่อ</a> ก่อน แล้วจึงเปิดใช้งานที่นี่ คุณสามารถกำหนดขอบเขตแต่ละรายการตามระดับต่ำสุดและสภาพแวดล้อม เพื่อไม่ให้เสียงรบกวนจาก dev ปะปนในช่อง prod
projects-integrations-deactivate-confirm = ปิดใช้งานการเชื่อมต่อนี้สำหรับโปรเจกต์หรือไม่
projects-integrations-deactivate = ปิดใช้งาน
projects-integrations-notify-new-issues = ปัญหาใหม่
projects-integrations-notify-regressions = การถดถอย
projects-integrations-notify-threshold = การแจ้งเตือนค่าขีดเริ่ม
projects-integrations-notify-digests = สรุป
projects-integrations-min-level = ระดับต่ำสุด
projects-integrations-level-any = ใดก็ได้
projects-integrations-env-filter = ตัวกรองสภาพแวดล้อม
projects-integrations-env-placeholder = เช่น production
projects-integrations-to-address = ที่อยู่ผู้รับ
projects-integrations-to-address-note = (เฉพาะการเชื่อมต่ออีเมล)
projects-integrations-activate-heading = เปิดใช้งานการเชื่อมต่อ
projects-integrations-integration-label = การเชื่อมต่อ
projects-integrations-activate-submit = เปิดใช้งาน
projects-integrations-available-empty = ไม่มีการเชื่อมต่อให้เลือก <a class="text-primary" href="/web/settings/integrations/">สร้างก่อน</a>

# --- สร้างโปรเจกต์แล้ว ---
projects-created-word = สร้างแล้ว
projects-created-breadcrumb = สร้างแล้ว
projects-created-heading = สร้างโปรเจกต์แล้ว
projects-created-subtitle = ใช้ DSN ด้านล่างเพื่อตั้งค่า SDK ของคุณ
projects-created-settings-btn = การตั้งค่าโปรเจกต์
projects-created-back = กลับไปที่โปรเจกต์
projects-created-details-heading = รายละเอียดโปรเจกต์
projects-created-col-id = รหัสโปรเจกต์
projects-created-sdk-desc-before = ติดตั้ง Sentry SDK สำหรับ
projects-created-sdk-desc-after = และเริ่มต้นด้วย DSN ด้านบน
projects-created-docs-javascript = เอกสาร Sentry JavaScript →
projects-created-docs-python = เอกสาร Sentry Python →
projects-created-docs-rust = เอกสาร Sentry Rust →
projects-created-docs-go = เอกสาร Sentry Go →
projects-created-docs-node = เอกสาร Sentry Node.js →
projects-created-docs-java = เอกสาร Sentry Java →
projects-created-docs-ruby = เอกสาร Sentry Ruby →
projects-created-docs-php = เอกสาร Sentry PHP →
projects-created-docs-elixir = เอกสาร Sentry Elixir →
projects-created-docs-dotnet = เอกสาร Sentry .NET →
projects-created-docs-apple = เอกสาร Sentry Apple →
projects-created-docs-kotlin = เอกสาร Sentry Kotlin →
projects-created-docs-native = เอกสาร Sentry Native →
projects-created-docs-generic = เอกสารแพลตฟอร์ม Sentry →
