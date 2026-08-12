# صفحة الخطأ المستقلة (src/html/mod.rs html_error) وصفحة تأكيد إنشاء الدعوة
# (src/html/orgs.rs). تبقى كلمة العلامة التجارية "Stackpit" حرفية في القوالب.
error-page-title = خطأ - Stackpit
error-heading = خطأ
error-not-found = الصفحة المطلوبة غير موجودة.
error-back-projects = العودة إلى المشاريع

# صفحة تأكيد إنشاء الدعوة (بالإنجليزية فقط، دون سياق طلب).
invite-created-page-title = تم إنشاء الدعوة - Stackpit
invite-created-heading = تم إنشاء الدعوة
invite-created-share = شارك هذا الرابط. صالح لمدة { $ttl } ويُستخدم مرة واحدة.
invite-created-back-members = العودة إلى الأعضاء

# --- رسائل الوميض / النجاح / التحقّق (حسب اللغة) ---
# تُبَثّ من معالجات الويب كنص لافتة لمرة واحدة. البادئة الديناميكية "خطأ: {e}"
# تُضاف في Rust عبر common-error-prefix.

# تشخيصات عدم العثور. تُضاف بادئة "خطأ:"/"Fehler:" في Rust؛ القيمة تحمل عبارة
# الكيان بالإضافة إلى المعرّف فقط.
flash-not-found-project = المشروع غير موجود: { $id }
flash-not-found-key = مفتاح API غير موجود: { $id }
flash-not-found-integration = التكامل غير موجود: { $id }
flash-not-found-alert-rule = قاعدة التنبيه غير موجودة: { $id }
flash-not-found-digest-schedule = جدول الملخّص غير موجود: { $id }
flash-not-found-repo = المستودع غير موجود: { $id }
flash-not-found-project-integration = تكامل المشروع غير موجود: { $id }
flash-not-found-filter = { $label } غير موجود

# التحقّق من قواعد الترشيح
flash-unrecognized-field = حقل غير معروف: { $value }
flash-unrecognized-operator = عامل غير معروف: { $value }
flash-unrecognized-action = إجراء غير معروف: { $value }

# إعدادات المشروع
flash-project-name-updated = تم تحديث اسم المشروع
flash-project-name-too-long = يتجاوز اسم المشروع الحدّ الأقصى البالغ { $max } حرفًا
flash-repo-url-required = عنوان URL للمستودع مطلوب
flash-repo-url-too-long = يتجاوز عنوان URL للمستودع الحدّ الأقصى البالغ 2048 حرفًا
flash-repo-added = تمت إضافة المستودع
flash-repo-removed = تمت إزالة المستودع
flash-project-archived = تمت أرشفة المشروع
flash-project-unarchived = تم إلغاء أرشفة المشروع
flash-key-created = تم إنشاء المفتاح
flash-key-deleted = تم حذف المفتاح

# التنبيهات والملخّصات
flash-project-not-found-or-denied = خطأ: المشروع غير موجود أو الوصول مرفوض
flash-alert-rule-created = تم إنشاء قاعدة التنبيه
flash-alert-rule-deleted = تم حذف قاعدة التنبيه
flash-digest-schedule-created = تم إنشاء جدول الملخّص
flash-digest-schedule-deleted = تم حذف جدول الملخّص

# تكاملات المشروع
flash-integration-not-found = التكامل غير موجود
flash-integration-activated = تم تفعيل التكامل
flash-integration-updated = تم تحديث التكامل
flash-integration-deactivated = تم تعطيل التكامل

# تكاملات المؤسسة
flash-name-required = الاسم مطلوب
flash-invalid-integration-kind = نوع تكامل غير صالح
flash-invalid-email-provider = مزوّد بريد إلكتروني غير صالح
flash-api-token-required = رمز API مطلوب.
flash-from-address-required = عنوان المرسِل مطلوب.
flash-smtp-not-configured = لم يتم تكوين SMTP. عيّن [email] host في إعدادات الخادم.
flash-invalid-to-address = يجب أن يكون المستلم عنوان بريد إلكتروني صالحًا.
flash-test-digest-sent = تم إدراج ملخص الاختبار في قائمة الانتظار لـ { $count } مشروع إلى تكاملاتها التي تدعم الملخصات.
flash-test-digest-sample = لا يوجد نشاط حديث، لذا تم إدراج ملخص عيّنة موسوم في قائمة الانتظار.
flash-test-digest-no-target = لا يوجد تكامل مفعّل به الملخصات لمشروع هذا الجدول.
flash-url-required = عنوان URL مطلوب
flash-secret-not-configured = تعذّر تخزين السر: التشفير غير مُهيّأ. اضبط STACKPIT_MASTER_KEY لتمكين تخزين الأسرار.
flash-integration-license-required = تتطلب تكاملات Slack والويب هوك وأنظمة تتبّع المهام ترخيصًا تجاريًا ساريًا. تبقى إشعارات البريد الإلكتروني متاحة دون ترخيص.
flash-integration-created = تم إنشاء التكامل
flash-integration-name-exists = يوجد تكامل بهذا الاسم بالفعل.
flash-integration-deleted = تم حذف التكامل
flash-integration-no-url = التكامل لا يحتوي على عنوان URL مُهيّأ
flash-test-notification-sent = تم إرسال إشعار الاختبار

# مرشّحات الوارد
flash-inbound-filters-updated = تم تحديث مرشّحات الوارد
flash-pattern-required = النمط مطلوب
flash-message-filter-added = تمت إضافة مرشّح الرسائل
flash-message-filter-removed = تمت إزالة مرشّح الرسائل
flash-rate-limit-updated = تم تحديث حدّ المعدّل
flash-environment-required = البيئة مطلوبة
flash-environment-excluded = تم استبعاد البيئة
flash-environment-filter-removed = تمت إزالة مرشّح البيئة
flash-release-filter-added = تمت إضافة مرشّح الإصدار
flash-release-filter-removed = تمت إزالة مرشّح الإصدار
flash-ua-filter-added = تمت إضافة مرشّح وكيل المستخدم
flash-ua-filter-removed = تمت إزالة مرشّح وكيل المستخدم
flash-rule-added = تمت إضافة القاعدة
flash-rule-removed = تمت إزالة القاعدة
flash-cidr-required = CIDR مطلوب
flash-invalid-cidr = تنسيق CIDR غير صالح
flash-ip-block-added = تمت إضافة حظر IP
flash-ip-block-removed = تمت إزالة حظر IP

# مشروع جديد
flash-project-name-required = اسم المشروع مطلوب
