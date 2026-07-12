# صفحة إعدادات التنبيهات والملخّصات (templates/alerts.html). تعيد استخدام
# nav-settings و nav-alerts-digests. يحتفظ alerts-page-title بكيان &amp;
# ويُعرض باستخدام |safe.
alerts-page-title = التنبيهات &amp; الملخّصات — Stackpit
alerts-notify-help-pre = تُطلَق الإشعارات عبر التكاملات في صفحة
alerts-notify-help-post = .

# --- أنواع الإشعارات ---
alerts-notify-types-heading = أنواع الإشعارات
alerts-notify-types-desc = تنطلق تنبيهات المشكلات الجديدة والانتكاسات مع كل مشكلة تُرى لأول مرة أو تعود من جديد، ويُتحكَّم بها لكل تكامل أدناه. تنطلق قواعد الحدّ بناءً على حجم الأحداث خلال نافذة زمنية؛ أمّا الملخّصات فهي تقارير دورية.
alerts-notify-types-empty = لا توجد تكاملات مشروع نشطة بعد. اربط واحدًا من صفحة تكاملات المشروع.
alerts-col-integration = التكامل
alerts-col-new-issues = مشكلات جديدة
alerts-col-regressions = الانتكاسات
alerts-col-digests = الملخّصات
alerts-notify-save = حفظ

# --- قواعد الحدّ ---
alerts-threshold-heading = قواعد الحدّ
alerts-threshold-desc = تُطلَق عندما تتلقّى مشكلة أكثر من N من الأحداث خلال نافذة زمنية.
alerts-rules-empty = لا توجد قواعد تنبيه بعد.
alerts-col-scope = النطاق
alerts-col-issue = المشكلة
alerts-col-threshold = الحدّ
alerts-col-window = النافذة
alerts-col-cooldown = فترة التهدئة
alerts-scope-global = عام
alerts-fingerprint-any = أي
alerts-rule-delete-confirm = هل تريد حذف قاعدة التنبيه هذه؟
alerts-delete-label = حذف
alerts-add-rule = + إضافة قاعدة تنبيه
alerts-all-projects = جميع المشاريع
alerts-project-fallback = المشروع { $id }
alerts-fingerprint-label = بصمة المشكلة
alerts-fingerprint-hint = (فارغ = أي)
alerts-fingerprint-placeholder = أي مشكلة
alerts-fingerprint-help = تحدّد البصمة مشكلة واحدة (أحداث مجمّعة). تظهر في عنوان URL على أي صفحة مشكلة. اتركها فارغة لمطابقة كل مشكلة ضمن النطاق.
alerts-unit-s = (ث)
alerts-create-rule = إنشاء قاعدة

# --- جداول الملخّصات ---
alerts-digest-heading = جداول الملخّصات
alerts-digest-desc = ملخّصات نشاط دورية — تقارير يومية أو أسبوعية بدلًا من ضجيج كل حدث على حدة.
alerts-digests-empty = لا توجد جداول ملخّصات بعد.
alerts-col-interval = الفترة
alerts-col-last-sent = آخر إرسال
alerts-col-enabled = مُفعّل
alerts-never = أبدًا
alerts-yes = نعم
alerts-no = لا
alerts-digest-delete-confirm = هل تريد حذف جدول الملخّص هذا؟
alerts-add-digest = + إضافة جدول ملخّص
alerts-interval-daily = يوميًا (24س)
alerts-interval-weekly = أسبوعيًا (7أيام)
alerts-interval-hourly = كل ساعة
alerts-create-schedule = إنشاء جدول
