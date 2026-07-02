# واجهة المؤسسات: قائمة المؤسسات (templates/orgs.html)، صفحة الأعضاء/الدعوات
# (templates/org_members.html)، وصفحة قبول الدعوة المستقلة
# (templates/invite_accept.html، مفاتيح invite-*). تعيد استخدام nav-organizations
# و common-action-save.
orgs-page-title = المؤسسات - Stackpit
orgs-subtitle = المؤسسات التي تنتمي إليها. بدّل بينها أو أنشئ واحدة جديدة.
orgs-empty = لست عضوًا في أي مؤسسة بعد.
orgs-col-organization = المؤسسة
orgs-col-kind = النوع
orgs-members-btn = الأعضاء
orgs-active = نشطة
orgs-switch = تبديل
orgs-create-heading = إنشاء مؤسسة
orgs-create-desc = تصبح المالك. يُشتقّ المُعرّف اللطيف من الاسم عند تركه فارغًا.
orgs-name = الاسم
orgs-slug = المُعرّف اللطيف
orgs-optional = (اختياري)
orgs-create-submit = إنشاء مؤسسة

# --- صفحة الأعضاء ---
orgs-members-title-suffix = الأعضاء - Stackpit
orgs-members-word = الأعضاء
orgs-organization-word = مؤسسة
orgs-slug-heading = المُعرّف اللطيف
orgs-slug-desc = يحدّد هذه المؤسسة في عناوين URL. يجب أن يكون فريدًا.
orgs-email = البريد الإلكتروني
orgs-role = الدور
orgs-role-member = عضو
orgs-role-owner = مالك
orgs-member-fallback = المستخدم #{ $id }
orgs-joined = تاريخ الانضمام
orgs-promote = ترقية
orgs-demote = خفض الرتبة
orgs-remove = إزالة
orgs-invites-heading = الدعوات
orgs-created = تاريخ الإنشاء
orgs-expires = تاريخ الانتهاء
orgs-status = الحالة
orgs-revoke = إبطال
orgs-create-invite-heading = إنشاء دعوة
orgs-create-invite-desc = يُنشئ رابط دعوة يُستخدم مرة واحدة.
orgs-expiry-label = مدة الانتهاء (بالثواني)
orgs-expiry-hint = (اختياري، الافتراضي 7 أيام)
orgs-create-invite-submit = إنشاء دعوة
orgs-forseti-note = تُدار عضوية هذه المؤسسة خارجيًا.
orgs-personal-note = هذه مؤسسة شخصية. العضوية غير قابلة للتهيئة.
orgs-danger-heading = منطقة الخطر
orgs-delete-danger-pre = يؤدي الحذف إلى إزالة
orgs-delete-danger-projects = مشروع (مشاريع)،
orgs-delete-danger-members = عضو (أعضاء)،
orgs-delete-danger-rest = وجميع الأحداث والمشكلات والمفاتيح والتنبيهات والتكاملات. لا يمكن التراجع عن ذلك.
orgs-confirm-type-pre = اكتب
orgs-confirm-type-post = للتأكيد
orgs-delete-confirm = احذف هذه المؤسسة وجميع بياناتها. لا يمكن التراجع عن ذلك.
orgs-delete-submit = حذف المؤسسة

# --- قبول الدعوة (صفحة مستقلة) ---
invite-page-title = دعوة مؤسسة - Stackpit
invite-heading = دعوة مؤسسة
invite-back-projects = العودة إلى المشاريع
invite-intro-pre = لقد دُعيت للانضمام إلى
invite-intro-as = بصفة
invite-intro-post = .
invite-accept-btn = قبول الدعوة
invite-decline = رفض
invite-error-accepted = تم قبول هذه الدعوة بالفعل.
invite-error-expired = انتهت صلاحية هذه الدعوة.

# رسائل التحقّق/الخطأ المعروضة في صفحة html_error، مترجمة عند مواضع الاستدعاء
# التي تحمل لغة الطلب. تبقى إخفاقات 5xx الداخلية بالإنجليزية.
orgs-err-name-required = اسم المؤسسة مطلوب.
orgs-err-slug-taken = هذا المُعرّف اللطيف مأخوذ بالفعل.
orgs-err-invite-not-found = الدعوة غير موجودة أو غير صالحة.
orgs-err-org-not-found = المؤسسة غير موجودة.
orgs-err-last-owner-remove = لا يمكن إزالة المالك الأخير.
orgs-err-last-owner-demote = لا يمكن خفض رتبة المالك الأخير.
orgs-err-confirm-slug = اكتب المُعرّف اللطيف للمؤسسة لتأكيد الحذف.
orgs-err-not-deletable = لا يمكن حذف هذه المؤسسة.
orgs-err-limit-reached = { $count ->
    [zero] لقد وصلت إلى الحدّ الأقصى البالغ { $count } مؤسسة.
    [one] لقد وصلت إلى الحدّ الأقصى البالغ مؤسسة واحدة.
    [two] لقد وصلت إلى الحدّ الأقصى البالغ مؤسستين.
    [few] لقد وصلت إلى الحدّ الأقصى البالغ { $count } مؤسسات.
    [many] لقد وصلت إلى الحدّ الأقصى البالغ { $count } مؤسسةً.
   *[other] لقد وصلت إلى الحدّ الأقصى البالغ { $count } مؤسسة.
}
