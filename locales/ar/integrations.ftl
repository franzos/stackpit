# واجهة إعدادات التكاملات: القائمة (templates/integrations.html) ونماذج
# "الإضافة" الثلاثة (webhook و slack و email). يحمل integrations-empty ترميز
# <strong> مضمّنًا ورمز السهم، ويُعرض باستخدام |safe.
integrations-page-title = التكاملات — Stackpit
integrations-subtitle = مخرجات Webhook و Slack والبريد الإلكتروني. يُضبط التوجيه لكل مشروع ضمن إعدادات كل مشروع.
integrations-add-webhook = + Webhook
integrations-add-slack = + Slack
integrations-add-email = + بريد إلكتروني
integrations-license-required-badge = يتطلب ترخيصًا
integrations-empty = لا توجد تكاملات بعد. أضف واحدًا أعلاه لبدء تلقّي الإشعارات. بعد الإضافة، فعّله لكل مشروع ضمن <strong>إعدادات المشروع ← التكاملات</strong>.
integrations-col-name = الاسم
integrations-col-type = النوع
integrations-col-endpoint = نقطة النهاية
integrations-col-created = تاريخ الإنشاء
integrations-delete-confirm = هل تريد حذف هذا التكامل؟ ستتم إزالته من جميع المشاريع.
integrations-test = اختبار
integrations-delete = حذف
flash-test-failed = فشل الاختبار: { $error }

# تسميات/أزرار مشتركة عبر نماذج إضافة التكامل الثلاثة.
integrations-cancel = إلغاء
integrations-optional = (اختياري)
integrations-required = (مطلوب)
integrations-create = إنشاء تكامل

# --- إضافة webhook ---
integrations-webhook-title = إضافة webhook — Stackpit
integrations-webhook-breadcrumb = إضافة webhook
integrations-webhook-heading = إضافة تكامل webhook
integrations-webhook-name-placeholder = مثال: تنبيهات الإنتاج
integrations-webhook-url-label = عنوان URL للـ Webhook
integrations-webhook-secret-label = سر HMAC
integrations-webhook-secret-placeholder = سر توقيع اختياري

# --- إضافة Slack ---
integrations-slack-title = إضافة Slack — Stackpit
integrations-slack-breadcrumb = إضافة Slack
integrations-slack-heading = إضافة تكامل Slack
integrations-slack-name-placeholder = مثال: قناة ‎#alerts
integrations-slack-url-label = عنوان URL للـ Webhook في Slack

# --- إضافة بريد إلكتروني ---
integrations-email-title = إضافة بريد إلكتروني — Stackpit
integrations-email-breadcrumb = إضافة بريد إلكتروني
integrations-email-heading = إضافة تكامل بريد إلكتروني
integrations-email-name-placeholder = مثال: تنبيهات بريد الفريق
integrations-email-lock-pre = يأتي المزوّد والمرسِل من إعدادات الخادم؛
integrations-email-lock-post = هذا التكامل يختار المستلِم فقط.
integrations-email-provider-label = المزوّد
integrations-email-token-label = رمز API
integrations-email-token-placeholder-default = اتركه فارغًا لاستخدام الافتراضي
integrations-email-token-placeholder = رمز API للمزوّد
integrations-email-from-label = عنوان المرسِل
integrations-email-fromname-label = اسم المرسِل
integrations-email-smtp-hint = يستخدم SMTP اتصال [email] الخاص بالخادم؛ لا حاجة إلى رمز لكل تكامل.

# متعقّب المشكلات
integrations-add-tracker = + متعقّب مشكلات
integrations-tracker-title = إضافة متعقّب مشكلات — Stackpit
integrations-tracker-breadcrumb = إضافة متعقّب مشكلات
integrations-tracker-heading = إضافة تكامل متعقّب مشكلات
integrations-tracker-kind-label = المتعقّب
integrations-tracker-name-placeholder = مثال: GitHub Issues
integrations-tracker-url-label = عنوان URL الأساسي
integrations-tracker-token-label = رمز API
integrations-tracker-token-placeholder = رمز وصول شخصي
integrations-tracker-target-help = المستودع الهدف يأتي من إعدادات مستودعات كل مشروع، لذلك لا يُضبط هنا. أضف المستودع من إعدادات المشروع.
integrations-global-label = التسليم إلى كل المشاريع
integrations-global-help = تذهب التنبيهات إلى كل مشروع في هذه المؤسسة، عدا ما تستبعده في صفحة هذا التكامل. تظل مرشّحات المستوى والبيئة الخاصة بكل مشروع سارية فوق ذلك.
integrations-global-badge = على مستوى المؤسسة
integrations-global-save = حفظ التوجيه
integrations-global-on = التسليم على مستوى المؤسسة
integrations-global-off = إيقاف التسليم على مستوى المؤسسة

# تفاصيل التكامل: التوجيه لكل مشروع
integrations-detail-title = التكامل — Stackpit
integrations-back = العودة إلى التكاملات
integrations-projects-heading = التوجيه لكل مشروع
integrations-projects-hint-global = يسلّم هذا التكامل إلى كل المشاريع أدناه ما لم تستبعدها. الاستبعاد هو السبيل الوحيد للخروج؛ لا توجد قائمة تضمين.
integrations-projects-hint-per-project = لا يسلّم هذا التكامل إلا حيث فعّله المشروع. اجعله على مستوى المؤسسة ليسلّم في كل مكان.
integrations-projects-hint-tracker = تُطابَق متعقّبات المشكلات مع مستودعات المشروع حسب نوع المنصّة والمضيف. استبعاد مشروع يُخرج هذا المتعقّب من خيارات الإنشاء فيه.
integrations-projects-empty = لا توجد مشاريع في هذه المؤسسة بعد.
integrations-col-project = المشروع
integrations-col-state = الحالة
integrations-project-archived = مؤرشف
integrations-state-default = يسلّم
integrations-state-customised = مخصّص
integrations-state-excluded = مستبعَد
integrations-state-no-repo = لا يوجد مستودع مطابق
integrations-state-not-routed = غير مُفعّل
integrations-exclude = استبعاد
integrations-include = تضمين
integrations-email-to-label = المستلم الافتراضي
integrations-email-to-help = يُستخدم حيث لم يضبط المشروع عنوانه الخاص. مطلوب للتكامل على مستوى المؤسسة.
