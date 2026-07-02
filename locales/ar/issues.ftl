# واجهة المشكلات: قائمة المشكلات المجمّعة حسب البصمة وصفحة تفاصيل المشكلة.
# يحمل issue-detail-exception-stacktrace كيان &amp; مضمّنًا ويُعرض باستخدام |safe.

# --- تسميات مشتركة (قائمة المشكلات + تفاصيل المشكلة) ---
issues-label-title = العنوان
issues-label-level = المستوى
issues-label-events = الأحداث
issues-label-users = المستخدمون
issues-label-status = الحالة
issues-label-first-seen = أول ظهور
issues-label-last-seen = آخر ظهور
issues-label-value = القيمة

# --- قيم الحالة (خيارات الترشيح + الشارات) ---
issues-status-unresolved = غير محلولة
issues-status-resolved = محلولة
issues-status-ignored = متجاهَلة

# --- ترقيم الصفحات (مشترك) ---
issues-pagination-label = ترقيم الصفحات
issues-pagination-prev = « السابق
issues-pagination-next = التالي »

# --- لاحقة عنوان الصفحة (عناوين ذات بادئة ديناميكية) ---
issues-title-suffix = — Stackpit

# --- قائمة المشكلات ---
issues-list-subtitle = المشكلات مجمّعة حسب البصمة.
issues-list-filtered-by-tag = مُرشّحة حسب الوسم:
issues-list-clear-tag = مسح ترشيح الوسم
issues-list-search-placeholder = ابحث في المشكلات…
issues-list-search-label = البحث في المشكلات
issues-list-select = تحديد مشكلة
issues-list-filter-status = ترشيح حسب الحالة
issues-list-status-all = جميع الحالات
issues-list-filter-level = ترشيح حسب المستوى
issues-list-level-all = جميع المستويات
issues-list-filter-release = ترشيح حسب الإصدار
issues-list-release-all = جميع الإصدارات
issues-period-label = النطاق الزمني
issues-period-all = كل الوقت
issues-period-1h = آخر ساعة
issues-period-24h = آخر 24 ساعة
issues-period-7d = آخر 7 أيام
issues-period-14d = آخر 14 يومًا
issues-period-30d = آخر 30 يومًا
issues-period-90d = آخر 90 يومًا
issues-period-365d = آخر 365 يومًا
issues-list-filter-submit = ترشيح
issues-list-empty = لا توجد مشكلات تطابق المرشّحات الحالية.
issues-untitled = (بلا عنوان)

# --- الإجراءات المجمّعة ---
issues-bulk-resolve-all = حلّ الكل ({ $count })
issues-bulk-ignore-all = تجاهل الكل ({ $count })
issues-bulk-delete-all = حذف الكل ({ $count })
issues-bulk-resolve-confirm = { $count ->
    [zero] هل تريد حلّ جميع المشكلات المطابقة؟
    [one] هل تريد حلّ المشكلة المطابقة؟
    [two] هل تريد حلّ المشكلتين المطابقتين؟
    [few] هل تريد حلّ جميع المشكلات المطابقة ({ $count })؟
    [many] هل تريد حلّ جميع المشكلات المطابقة ({ $count })؟
   *[other] هل تريد حلّ جميع المشكلات المطابقة ({ $count })؟
}
issues-bulk-ignore-confirm = { $count ->
    [zero] هل تريد تجاهل جميع المشكلات المطابقة؟
    [one] هل تريد تجاهل المشكلة المطابقة؟
    [two] هل تريد تجاهل المشكلتين المطابقتين؟
    [few] هل تريد تجاهل جميع المشكلات المطابقة ({ $count })؟
    [many] هل تريد تجاهل جميع المشكلات المطابقة ({ $count })؟
   *[other] هل تريد تجاهل جميع المشكلات المطابقة ({ $count })؟
}
issues-bulk-delete-all-confirm = { $count ->
    [zero] هل تريد حذف جميع المشكلات المطابقة نهائيًا؟
    [one] هل تريد حذف المشكلة المطابقة نهائيًا؟
    [two] هل تريد حذف المشكلتين المطابقتين نهائيًا؟
    [few] هل تريد حذف جميع المشكلات المطابقة ({ $count }) نهائيًا؟
    [many] هل تريد حذف جميع المشكلات المطابقة ({ $count }) نهائيًا؟
   *[other] هل تريد حذف جميع المشكلات المطابقة ({ $count }) نهائيًا؟
}
issues-bulk-resolve = حلّ
issues-bulk-ignore = تجاهل
issues-bulk-delete = حذف
issues-bulk-delete-selected-confirm = هل تريد حذف المشكلات المحدّدة نهائيًا؟

# --- العدد (ترقيم الصفحات) ---
issues-count = { $count ->
    [zero] لا مشكلات
    [one] مشكلة واحدة
    [two] مشكلتان
    [few] { $count } مشكلات
    [many] { $count } مشكلةً
   *[other] { $count } مشكلة
}

# --- تفاصيل المشكلة ---
issue-detail-title-fallback = المشكلة
issue-detail-resolve = ✓ حلّ
issue-detail-reopen = إعادة فتح
issue-detail-unignore = إلغاء التجاهل
issue-detail-tab-details = التفاصيل
issue-detail-tab-events = جميع الأحداث
issue-detail-exception-stacktrace = الاستثناء &amp; تتبّع المكدّس
issue-detail-handled = مُعالَج
issue-detail-unhandled = غير مُعالَج
issue-detail-in = في
issue-detail-var-name = المتغيّر
issue-detail-no-source = لا يتوفّر سياق المصدر
issue-detail-breadcrumbs = مسارات التنقّل
issue-detail-th-time = الوقت
issue-detail-th-category = الفئة
issue-detail-th-message = الرسالة
issue-detail-crumb-data = بيانات
issue-detail-tags = الوسوم
issue-detail-contexts = السياقات
issue-detail-request = الطلب
issue-detail-headers = الترويسات
issue-detail-th-header = الترويسة
issue-detail-query-string = سلسلة الاستعلام
issue-detail-body = المحتوى
issue-detail-environment = البيئة
issue-detail-user-reports = تقارير المستخدمين
issue-detail-anonymous = مجهول
issue-detail-attachments = المرفقات
issue-detail-att-filename = اسم الملف
issue-detail-att-type = النوع
issue-detail-att-size = الحجم
issue-detail-download = تنزيل
issue-detail-raw-json = JSON الخام
issue-detail-no-events = لم يُعثر على أحداث لهذه المشكلة.
issue-detail-ev-id = معرّف الحدث
issue-detail-ev-timestamp = الطابع الزمني
issue-detail-ev-platform = المنصّة
issue-detail-events-count = { $count ->
    [zero] لا أحداث
    [one] حدث واحد
    [two] حدثان
    [few] { $count } أحداث
    [many] { $count } حدثًا
   *[other] { $count } حدث
}
issue-detail-props-heading = خصائص المشكلة
issue-detail-fingerprint = البصمة
issue-detail-tag-facets = أوجه الوسوم
issue-detail-discard-undo-title = استئناف قبول الأحداث المستقبلية بهذه البصمة
issue-detail-discard-undo = التراجع عن التجاهل
issue-detail-discard-confirm = هل تريد تجاهل جميع الأحداث المستقبلية بهذه البصمة؟
issue-detail-discard-title = إسقاط الأحداث المستقبلية المطابقة لهذه البصمة بصمت
issue-detail-discard = تجاهل الأحداث المستقبلية
