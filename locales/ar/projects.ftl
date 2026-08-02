# واجهة المشاريع: القائمة، الإنشاء، الإعدادات (عامة/مفاتيح/خرائط المصدر/مرشّحات)،
# التكاملات، وصفحة تأكيد الإنشاء. القيم المعروضة باستخدام |safe تحمل ترميز HTML
# مضمّنًا كما هو موضّح أدناه.

# --- قائمة المشاريع ---
projects-list-title = المشاريع — Stackpit
projects-list-heading = المشاريع
projects-list-subtitle = راقب الصحة عبر بنيتك بالكامل.
projects-list-all-events = جميع الأحداث
projects-list-all-releases = جميع الإصدارات
projects-list-new = + مشروع جديد
projects-list-search-placeholder = ابحث في المشاريع بالاسم أو المنصّة أو المالك…
projects-list-search-label = البحث في المشاريع
projects-list-filter = ترشيح
projects-org-filter-label = ترشيح حسب المؤسسة
projects-org-filter-all = كل المؤسسات
projects-list-empty = لم يُعثر على مشاريع. ستظهر الأحداث هنا بمجرّد استيعابها.
projects-period-label = النطاق الزمني
projects-period-all = كل الوقت
projects-period-1h = آخر ساعة
projects-period-24h = آخر 24 ساعة
projects-period-7d = آخر 7 أيام
projects-period-14d = آخر 14 يومًا
projects-period-30d = آخر 30 يومًا
projects-period-90d = آخر 90 يومًا
projects-period-365d = آخر 365 يومًا
projects-col-project = المشروع
projects-col-platforms = المنصّات
projects-col-issues = المشكلات
projects-col-events = الأحداث
projects-col-breakdown = التفصيل
projects-col-release = الإصدار
projects-col-first-seen = أول ظهور
projects-col-last-seen = آخر ظهور
projects-breakdown-errors = الأخطاء:
projects-breakdown-transactions = المعاملات:
projects-breakdown-sessions = الجلسات:
projects-breakdown-other = أخرى:
projects-legend-errors = الأخطاء
projects-legend-transactions = المعاملات
projects-legend-sessions = الجلسات
projects-legend-other = أخرى

# --- مشترك عبر نماذج المشروع ---
projects-optional = (اختياري)
projects-cancel = إلغاء
projects-remove = إزالة
projects-delete = حذف
projects-name-placeholder = مشروعي

# --- مشروع جديد ---
projects-new-title = مشروع جديد — Stackpit
projects-new-heading = مشروع جديد
projects-new-name-label = اسم المشروع
projects-new-platform-label = المنصّة
projects-new-platform-select = اختر منصّة…
projects-new-platform-other = أخرى
projects-new-platform-native = Native (C/C++)
projects-new-submit = إنشاء مشروع

# --- علامات تبويب الإعدادات (مشتركة بين صفحات الإعدادات) ---
projects-tab-general = عام
projects-tab-sdk = إعداد SDK
projects-tab-sourcemaps = خرائط المصدر
projects-tab-filters = المرشّحات
projects-tab-integrations = التكاملات

# --- الإعدادات: عام ---
projects-settings-heading = الإعدادات
projects-settings-archived = (مؤرشف)
projects-settings-name-heading = اسم المشروع
projects-settings-display-name = الاسم المعروض
projects-settings-save-name = حفظ الاسم
projects-settings-info-heading = معلومات المشروع
projects-settings-status = الحالة
projects-settings-source = المصدر
projects-repos-heading = مستودعات المصدر
projects-repos-help = اربط إطارات المكدّس بالشيفرة المصدرية على منصّتك. سجّل إصدارًا مع SHA لالتزام عبر <code class="text-mono">sentry-cli</code> لتفعيل الروابط.
projects-repos-empty = لا توجد مستودعات مُهيّأة.
projects-repos-url-label = عنوان URL للمستودع
projects-repos-col-forge = المنصّة
projects-repos-template = قالب URL
projects-repos-auto = تلقائي
projects-repos-remove-confirm = هل تريد إزالة هذا المستودع؟
projects-repos-add = إضافة مستودع
projects-repos-add-help = يضيف روابط مصدر قابلة للنقر (مثل "عرض على GitHub") بجانب إطارات المكدّس. يتطلّب إصدارًا مع SHA لالتزام — يُكتشف نوع المنصّة تلقائيًا. المدعومة: GitHub و GitLab و Gitea/Codeberg و Bitbucket و Sourcehut و Gitee و Azure DevOps. للمنصّات الأخرى، وفّر قالب URL.
projects-danger-heading = منطقة الخطر
projects-archive-desc = أرشِف هذا المشروع. المشاريع المؤرشفة ترفض الأحداث الجديدة.
projects-archive-confirm = هل تريد أرشفة هذا المشروع؟ سيتم رفض الأحداث الجديدة.
projects-archive-submit = أرشفة المشروع
projects-unarchive-desc = ألغِ أرشفة هذا المشروع لاستئناف قبول الأحداث.
projects-unarchive-submit = إلغاء أرشفة المشروع
projects-delete-desc = احذف هذا المشروع وجميع بياناته نهائيًا. لا يمكن التراجع عن ذلك.
projects-delete-confirm = هل تريد حذف هذا المشروع وجميع بياناته؟ لا يمكن التراجع عن ذلك.
projects-delete-submit = حذف المشروع
projects-move-heading = النقل إلى مؤسسة أخرى
projects-move-desc = انقل هذا المشروع إلى مؤسسة أخرى تملكها. تبقى بياناته وعناوين DSN صالحة، لكن يتم فصل تكاملات الإشعارات ويجب إعادة إضافتها في المؤسسة الجديدة.
projects-move-target-label = المؤسسة الوجهة
projects-move-confirm-pre = اكتب
projects-move-confirm-post = للتأكيد.
projects-move-confirm-placeholder = اسم المشروع
projects-move-confirm-dialog = نقل هذا المشروع إلى المؤسسة المحددة؟
projects-move-submit = نقل المشروع
projects-move-err-invalid-target = المؤسسة الوجهة غير صالحة.
projects-move-err-name-mismatch = اسم المشروع غير مطابق.
projects-move-err-denied = لست مالكًا للمؤسسة الوجهة.
projects-move-err-conflict = تعذّر نقل المشروع؛ ربما تغيّر. حاول مرة أخرى.

# --- الإعدادات: إعداد SDK / المفاتيح ---
projects-keys-title = إعداد SDK
projects-keys-dsn-heading = DSN
projects-keys-dsn-empty = لا توجد مفاتيح مسجّلة. أنشئ مفتاحًا أدناه للحصول على DSN.
projects-keys-list-heading = مفاتيح المشروع
projects-keys-empty = لا توجد مفاتيح مسجّلة لهذا المشروع.
projects-keys-col-public = المفتاح العام
projects-keys-col-label = التسمية
projects-keys-col-status = الحالة
projects-keys-col-created = تاريخ الإنشاء
projects-keys-delete-confirm = هل تريد حذف هذا المفتاح؟ ستتوقّف SDKs التي تستخدمه عن العمل.
projects-keys-create-heading = إنشاء مفتاح
projects-keys-label-label = التسمية
projects-keys-label-placeholder = مثال: production، staging
projects-keys-create-submit = إنشاء مفتاح

# --- الإعدادات: خرائط المصدر ---
projects-sourcemaps-title = خرائط المصدر
projects-sourcemaps-apikey-heading = مفتاح API
projects-sourcemaps-apikey-desc = يتطلّب رفع خرائط المصدر مفتاح API. خاص بهذا المشروع وقابل للاستخدام في عمليات خرائط المصدر فقط.
projects-sourcemaps-key-generated = تم إنشاء المفتاح:
projects-sourcemaps-key-warning = انسخ هذا المفتاح الآن — لن يُعرض مرة أخرى.
projects-sourcemaps-col-key = المفتاح
projects-sourcemaps-regen-confirm = هل تريد إعادة إنشاء المفتاح؟ سيتوقّف المفتاح الحالي عن العمل.
projects-sourcemaps-regen = إعادة إنشاء
projects-sourcemaps-empty = لا يوجد مفتاح API لخرائط المصدر لهذا المشروع.
projects-sourcemaps-generate = إنشاء مفتاح
projects-sourcemaps-setup-heading = الإعداد
projects-sourcemaps-setup-desc = استخدم <a class="text-primary" href="https://docs.sentry.io/cli/" rel="noopener noreferrer">sentry-cli</a> لرفع خرائط المصدر. اضبط متغيّرات البيئة التالية:
projects-sourcemaps-then-upload = ثم ارفع:

# --- الإعدادات: المرشّحات ---
projects-filters-inbound-heading = مرشّحات الوارد
projects-filters-inbound-desc = مرشّحات مدمجة تُسقِط الأحداث المطابقة لأنماط الضجيج الشائعة.
projects-filters-browser-ext = إضافات المتصفّح — إسقاط الأحداث من إضافات Chrome/Firefox/Safari
projects-filters-localhost = Localhost — إسقاط الأحداث من localhost و 127.0.0.1 وعناوين IP الخاصة
projects-filters-inbound-submit = حفظ مرشّحات الوارد
projects-filters-message-heading = مرشّحات الرسائل
projects-filters-message-help = أنماط glob تُطابَق مع عناوين الأحداث. استخدم <code class="text-mono">*</code> لأي تسلسل، و <code class="text-mono">?</code> لحرف واحد.
projects-filters-col-pattern = النمط
projects-filters-message-empty = لا توجد مرشّحات رسائل مُهيّأة.
projects-filters-add-pattern = إضافة نمط
projects-filters-message-submit = إضافة مرشّح رسائل
projects-filters-ratelimit-heading = حدّ المعدّل
projects-filters-ratelimit-desc = الحدّ الأقصى للأحداث في الدقيقة لهذا المشروع. 0 = بلا حدّ.
projects-filters-ratelimit-label = الأحداث في الدقيقة
projects-filters-ratelimit-submit = حفظ حدّ المعدّل
projects-filters-env-heading = البيئات المستبعَدة
projects-filters-env-desc = الأحداث من هذه البيئات ستُسقَط بصمت.
projects-filters-col-environment = البيئة
projects-filters-env-empty = لا توجد بيئات مستبعَدة.
projects-filters-env-add-label = إضافة بيئة مستبعَدة
projects-filters-env-submit = استبعاد بيئة
projects-filters-release-heading = مرشّحات الإصدار
projects-filters-release-desc = أنماط glob تُطابَق مع نسخ الإصدارات. تُسقَط الأحداث المطابقة.
projects-filters-release-empty = لا توجد مرشّحات إصدار.
projects-filters-release-submit = إضافة مرشّح إصدار
projects-filters-ua-heading = مرشّحات وكيل المستخدم
projects-filters-ua-desc = أنماط glob تُطابَق مع ترويسات User-Agent. الأنماط المدمجة لـ kube-probe وفاحصي الصحة نشطة دائمًا.
projects-filters-ua-empty = لا توجد مرشّحات وكيل مستخدم مخصّصة.
projects-filters-ua-submit = إضافة مرشّح وكيل مستخدم
projects-filters-rules-heading = قواعد مخصّصة
projects-filters-rules-desc = قواعد متقدّمة تطابق حقول الأحداث. تُقيَّم القواعد ذات الأولوية الأعلى أولًا.
projects-filters-col-field = الحقل
projects-filters-col-operator = العامل
projects-filters-col-value = القيمة
projects-filters-col-action = الإجراء
projects-filters-col-priority = الأولوية
projects-filters-rules-empty = لا توجد قواعد مخصّصة.
projects-filters-sample-rate-label = معدّل العيّنة
projects-filters-sample-rate-range = (0.0–1.0)
projects-filters-rules-submit = إضافة قاعدة
projects-filters-op = { $op ->
    [not_equals] لا يساوي
    [contains] يحتوي على
    [not_contains] لا يحتوي على
    [starts_with] يبدأ بـ
    [in] ضمن
    [not_in] ليس ضمن
   *[equals] يساوي
}
projects-filters-action = { $action ->
    [sample] أخذ عيّنة
   *[drop] إسقاط
}
projects-filters-ip-heading = قائمة حظر IP
projects-filters-ip-desc = كتل CIDR أو عناوين IP فردية. تُسقَط الأحداث من عناوين IP المحظورة بصمت.
projects-filters-col-cidr = CIDR
projects-filters-ip-empty = لا توجد كتل IP مُهيّأة.
projects-filters-ip-add-label = إضافة CIDR
projects-filters-ip-submit = حظر نطاق IP
projects-filters-discard-heading = إحصاءات التجاهل
projects-filters-discard-window = (آخر 7 أيام)
projects-filters-col-date = التاريخ
projects-filters-col-reason = السبب
projects-filters-col-count = العدد

# تسميات كيانات المرشّح، تُدرَج في flash-not-found-filter عند الحذف.
projects-filter-label-message = مرشّح الرسائل
projects-filter-label-environment = مرشّح البيئة
projects-filter-label-release = مرشّح الإصدار
projects-filter-label-user-agent = مرشّح وكيل المستخدم
projects-filter-label-rule = قاعدة المرشّح

# --- الإعدادات: التكاملات ---
projects-integrations-active-heading = التكاملات النشطة
projects-integrations-active-empty = لا توجد تكاملات مفعّلة. أضف تكاملًا عامًا في صفحة <a class="text-primary" href="/web/settings/integrations/">التكاملات</a> أولًا، ثم فعّله هنا. يمكنك تحديد نطاق كل تكامل بالحدّ الأدنى للمستوى والبيئة لإبقاء ضجيج التطوير خارج قنوات الإنتاج.
projects-integrations-deactivate-confirm = هل تريد تعطيل هذا التكامل للمشروع؟
projects-integrations-deactivate = تعطيل
projects-integrations-notify-new-issues = المشكلات الجديدة
projects-integrations-notify-regressions = الانحدارات
projects-integrations-notify-threshold = تنبيهات الحدّ
projects-integrations-notify-digests = الملخّصات
projects-integrations-min-level = الحدّ الأدنى للمستوى
projects-integrations-level-any = أي
projects-integrations-env-filter = مرشّح البيئة
projects-integrations-env-placeholder = مثال: production
projects-integrations-to-address = عنوان المستلِم
projects-integrations-to-address-note = (تكاملات البريد الإلكتروني فقط)
projects-integrations-activate-heading = تفعيل تكامل
projects-integrations-integration-label = التكامل
projects-integrations-activate-submit = تفعيل
projects-integrations-available-empty = لا توجد تكاملات متاحة. <a class="text-primary" href="/web/settings/integrations/">أنشئ واحدًا أولًا</a>.

# --- تم إنشاء المشروع ---
projects-created-word = مُنشأ
projects-created-breadcrumb = مُنشأ
projects-created-heading = تم إنشاء المشروع
projects-created-subtitle = استخدم DSN أدناه لتهيئة SDK الخاص بك.
projects-created-settings-btn = إعدادات المشروع
projects-created-back = العودة إلى المشاريع
projects-created-details-heading = تفاصيل المشروع
projects-created-col-id = معرّف المشروع
projects-created-sdk-desc-before = ثبّت Sentry SDK لـ
projects-created-sdk-desc-after = وهيّئه باستخدام DSN أعلاه.
projects-created-docs-javascript = توثيق Sentry JavaScript ←
projects-created-docs-python = توثيق Sentry Python ←
projects-created-docs-rust = توثيق Sentry Rust ←
projects-created-docs-go = توثيق Sentry Go ←
projects-created-docs-node = توثيق Sentry Node.js ←
projects-created-docs-java = توثيق Sentry Java ←
projects-created-docs-ruby = توثيق Sentry Ruby ←
projects-created-docs-php = توثيق Sentry PHP ←
projects-created-docs-elixir = توثيق Sentry Elixir ←
projects-created-docs-dotnet = توثيق Sentry .NET ←
projects-created-docs-apple = توثيق Sentry Apple ←
projects-created-docs-kotlin = توثيق Sentry Kotlin ←
projects-created-docs-native = توثيق Sentry Native ←
projects-created-docs-generic = توثيق منصّة Sentry ←
