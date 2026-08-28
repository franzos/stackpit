# واجهة الإعدادات: صفحة الإعدادات الافتراضية للمتصفّح
# (templates/browser_defaults.html، مفاتيح defaults-*) وصفحة تهيئة المؤسسات
# المستقلة (templates/provision.html، مفاتيح provision-*). تعيد استخدام
# nav-settings. تبقى قيم المستوى (fatal/error/...) حرفية في القالب.

# --- الإعدادات الافتراضية للمتصفّح ---
defaults-page-title = الإعدادات الافتراضية للمتصفّح — Stackpit
defaults-subtitle = اضبط قيم الترشيح الافتراضية لصفحات القوائم. تُخزَّن كملف تعريف ارتباط في المتصفّح.
defaults-none = بلا افتراضي
defaults-status-label = الحالة الافتراضية (المشكلات)
defaults-status-unresolved = غير محلولة
defaults-status-resolved = محلولة
defaults-status-ignored = متجاهَلة
defaults-level-label = المستوى الافتراضي
defaults-period-label = النطاق الزمني الافتراضي
defaults-save = حفظ الإعدادات الافتراضية
defaults-clear-confirm = هل تريد مسح جميع الإعدادات الافتراضية للمتصفّح؟
defaults-clear = مسح جميع الإعدادات الافتراضية
flash-defaults-saved = تم حفظ الإعدادات الافتراضية
flash-defaults-cleared = تم مسح الإعدادات الافتراضية

# --- اللغة المفضّلة ---
settings-language-heading = اللغة المفضّلة
settings-language-subtitle = اختر لغة واجهة Stackpit. تحتفظ الحسابات المسجَّل دخولها بها عبر الأجهزة.
settings-language-label = اللغة
settings-language-save = حفظ اللغة

settings-aria-sections = أقسام الإعدادات

# --- صفحة تهيئة المؤسسات (صفحة مستقلة) ---
provision-page-title = إعداد المؤسسات — Stackpit
provision-heading = إعداد المؤسسات
provision-subtitle-1 = المؤسسات التالية متاحة من مزوّد الهوية الخاص بك.
provision-subtitle-2 = اختر المؤسسات التي تريد إنشاءها في Stackpit.
provision-create = إنشاء المحدّد
provision-skip = تخطّي

# طابور التسليم
queue-page-title = طابور التسليم — Stackpit
queue-subtitle = إشعارات تعذّر تسليمها. تُعاد المحاولة تلقائيًا لمدة 24 ساعة، ثم تنتظرك هنا.
queue-count-pending = { $count } قيد الانتظار
queue-count-failed = { $count } فاشلة
queue-empty = لا شيء في الطابور. سُلّمت كل الإشعارات.
queue-col-integration = التكامل
queue-col-project = المشروع
queue-col-state = الحالة
queue-col-attempts = المحاولات
queue-col-queued = في الطابور منذ
queue-col-error = آخر خطأ
queue-state-pending = إعادة المحاولة
queue-state-failed = تم التخلي
queue-replay = إعادة الإرسال
queue-cancel = تجاهل
queue-cancel-confirm = تجاهل هذا الإشعار دون تسليمه؟
queue-col-alert = التنبيه
