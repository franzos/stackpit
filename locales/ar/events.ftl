# واجهة الأحداث: قائمة الأحداث عبر المشاريع وصفحة تفاصيل الحدث.
# يحمل event-detail-exception-stacktrace كيان &amp; مضمّنًا ويُعرض باستخدام |safe.

# --- تسميات مشتركة (قائمة الأحداث + تفاصيل الحدث) ---
events-label-title = العنوان
events-label-type = النوع
events-label-level = المستوى
events-label-platform = المنصّة
events-label-environment = البيئة
events-label-time = الوقت
events-label-value = القيمة

# --- ترقيم الصفحات (مشترك) ---
events-pagination-label = ترقيم الصفحات
events-pagination-prev = « السابق
events-pagination-next = التالي »

# --- لاحقة عنوان الصفحة (عناوين ذات بادئة ديناميكية) ---
events-title-suffix = — Stackpit

# --- قائمة الأحداث ---
events-list-title = الأحداث — Stackpit
events-heading = الأحداث
events-list-search-placeholder = ابحث في الأحداث…
events-list-search-label = البحث في الأحداث
events-list-select = تحديد حدث
events-list-filter-level = ترشيح حسب المستوى
events-list-level-all = جميع المستويات
events-list-filter-type = ترشيح حسب النوع
events-list-type-all = جميع الأنواع
events-list-project-placeholder = معرّف المشروع
events-list-filter-project = ترشيح حسب المشروع
events-list-filter-submit = ترشيح
events-list-empty = لا توجد أحداث تطابق المرشّحات الحالية.
events-untitled = (بلا عنوان)
events-col-project = المشروع

# --- الإجراءات المجمّعة ---
events-bulk-delete = حذف
events-bulk-delete-selected-confirm = هل تريد حذف الأحداث المحدّدة؟
events-bulk-delete-all = حذف كل المطابقة ({ $count })
events-bulk-delete-all-confirm = { $count ->
    [zero] هل تريد حذف جميع الأحداث المطابقة نهائيًا؟
    [one] هل تريد حذف الحدث المطابق نهائيًا؟
    [two] هل تريد حذف الحدثين المطابقين نهائيًا؟
    [few] هل تريد حذف جميع الأحداث المطابقة ({ $count }) نهائيًا؟
    [many] هل تريد حذف جميع الأحداث المطابقة ({ $count }) نهائيًا؟
   *[other] هل تريد حذف جميع الأحداث المطابقة ({ $count }) نهائيًا؟
}

# --- العدد (ترقيم الصفحات) ---
events-count = { $count ->
    [zero] لا أحداث
    [one] حدث واحد
    [two] حدثان
    [few] { $count } أحداث
    [many] { $count } حدثًا
   *[other] { $count } حدث
}

# --- تفاصيل الحدث ---
event-detail-event = الحدث
event-detail-event-id-label = event_id:
event-detail-nav-label = التنقّل بين الأحداث
event-detail-nav-newer = « أحدث
event-detail-nav-older = أقدم »
event-detail-nav-count = { $count ->
    [zero] لا أحداث
    [one] حدث واحد
    [two] حدثان
    [few] { $count } أحداث
    [many] { $count } حدثًا
   *[other] { $count } حدث
}
event-detail-nav-in-issue = في المشكلة
event-detail-user-feedback = ملاحظات المستخدم
event-detail-anonymous = مجهول
event-detail-related-event = حدث ذو صلة:
event-detail-exception-stacktrace = الاستثناء &amp; تتبّع المكدّس
event-detail-handled = مُعالَج
event-detail-unhandled = غير مُعالَج
event-detail-in = في
event-detail-var-name = المتغيّر
event-detail-no-source = لا يتوفّر سياق المصدر
event-detail-breadcrumbs = مسارات التنقّل
event-detail-th-category = الفئة
event-detail-th-message = الرسالة
event-detail-tags = الوسوم
event-detail-contexts = السياقات
event-detail-request = الطلب
event-detail-headers = الترويسات
event-detail-th-header = الترويسة
event-detail-query-string = سلسلة الاستعلام
event-detail-body = المحتوى
event-detail-user-reports = تقارير المستخدمين
event-detail-attachments = المرفقات
event-detail-att-filename = اسم الملف
event-detail-att-size = الحجم
event-detail-download = تنزيل
event-detail-web-vitals = Web Vitals
event-detail-raw-json = JSON الخام
event-detail-props-heading = خصائص الحدث
event-detail-prop-event-id = معرّف الحدث
event-detail-prop-timestamp = الطابع الزمني
event-detail-prop-transaction = المعاملة
event-detail-prop-release = الإصدار
event-detail-prop-server = الخادم
event-detail-prop-sdk = SDK
event-detail-prop-received = تاريخ الاستلام
event-detail-user-heading = المستخدم
event-detail-user-id = المعرّف
event-detail-user-email = البريد الإلكتروني
event-detail-user-username = اسم المستخدم
event-detail-user-ip = عنوان IP

# --- تقارير العميل (نتائج الأحداث المُسقَطة) ---
client-reports-title = تقارير العميل
client-reports-heading = تقارير العميل
client-reports-dropped-heading = الأحداث المُسقَطة
client-reports-dropped-subtitle = ما تجاهلته SDKs قبل الإرسال، حسب الفئة والسبب.
client-reports-th-category = الفئة
client-reports-th-reason = السبب
client-reports-th-dropped = مُسقَط
client-reports-empty = لم يُعثر على تقارير عميل لهذا المشروع.
client-reports-reports-heading = التقارير
client-reports-delete = حذف
client-reports-delete-selected-confirm = هل تريد حذف التقارير المحدّدة؟
client-reports-th-event-id = معرّف الحدث
client-reports-th-title = العنوان
client-reports-th-timestamp = الطابع الزمني
client-reports-th-platform = المنصّة
client-reports-th-release = الإصدار
client-reports-select = تحديد تقرير
client-reports-delete-all = حذف الكل ({ $count })
client-reports-delete-all-confirm = { $count ->
    [zero] هل تريد حذف جميع التقارير المطابقة؟
    [one] هل تريد حذف التقرير المطابق؟
    [two] هل تريد حذف التقريرين المطابقين؟
    [few] هل تريد حذف جميع التقارير المطابقة ({ $count })؟
    [many] هل تريد حذف جميع التقارير المطابقة ({ $count })؟
   *[other] هل تريد حذف جميع التقارير المطابقة ({ $count })؟
}
client-reports-count = { $count ->
    [zero] لا تقارير
    [one] تقرير واحد
    [two] تقريران
    [few] { $count } تقارير
    [many] { $count } تقريرًا
   *[other] { $count } تقرير
}

# --- تقارير المستخدمين (ملاحظات المستخدم) ---
user-reports-title = تقارير المستخدمين
user-reports-heading = تقارير المستخدمين
user-reports-empty = لم يُعثر على تقارير مستخدمين لهذا المشروع.
user-reports-delete = حذف
user-reports-delete-selected-confirm = هل تريد حذف التقارير المحدّدة؟
user-reports-th-event-id = معرّف الحدث
user-reports-th-title = العنوان
user-reports-th-timestamp = الطابع الزمني
user-reports-th-platform = المنصّة
user-reports-th-release = الإصدار
user-reports-select = تحديد تقرير
user-reports-delete-all = حذف الكل ({ $count })
user-reports-delete-all-confirm = { $count ->
    [zero] هل تريد حذف جميع التقارير المطابقة؟
    [one] هل تريد حذف التقرير المطابق؟
    [two] هل تريد حذف التقريرين المطابقين؟
    [few] هل تريد حذف جميع التقارير المطابقة ({ $count })؟
    [many] هل تريد حذف جميع التقارير المطابقة ({ $count })؟
   *[other] هل تريد حذف جميع التقارير المطابقة ({ $count })؟
}
user-reports-count = { $count ->
    [zero] لا تقارير
    [one] تقرير واحد
    [two] تقريران
    [few] { $count } تقارير
    [many] { $count } تقريرًا
   *[other] { $count } تقرير
}
