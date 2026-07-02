# واجهة الامتدادات: قائمة الامتدادات/التتبّعات لكل مشروع (spans-*) وصفحة تفاصيل
# شلال التتبّع (trace-detail-*). تعيد استخدام nav-spans.

# --- لاحقة عنوان الصفحة ---
spans-title-suffix = — Stackpit

# --- قائمة الامتدادات/التتبّعات ---
spans-list-empty = لم يُعثر على امتدادات لهذا المشروع.
spans-traces-heading = التتبّعات
spans-all-heading = جميع الامتدادات

# --- جدول التتبّعات ---
spans-col-trace-id = معرّف التتبّع
spans-col-root-op = العملية الجذرية
spans-col-root-description = الوصف الجذري
spans-col-duration = المدّة
spans-col-first-seen = أول ظهور
spans-col-last-seen = آخر ظهور

# --- جدول جميع الامتدادات ---
spans-col-span-id = معرّف الامتداد
spans-col-op = العملية
spans-col-description = الوصف
spans-col-timestamp = الطابع الزمني

# --- ترقيم الصفحات (قائمة الامتدادات) ---
spans-pagination-label = ترقيم الصفحات
spans-pagination-prev = « السابق
spans-pagination-next = التالي »
spans-count = { $count ->
    [zero] لا امتدادات
    [one] امتداد واحد
    [two] امتدادان
    [few] { $count } امتدادات
    [many] { $count } امتدادًا
   *[other] { $count } امتداد
}

# --- تفاصيل التتبّع (الشلال) ---
trace-detail-title-prefix = التتبّع
trace-detail-title-suffix = — Stackpit
trace-detail-trace-id-label = trace_id:
trace-detail-total = الإجمالي
trace-detail-showing-first = عرض أول
trace-detail-of = من
trace-detail-empty = لم يُعثر على امتدادات لهذا التتبّع.
trace-detail-col-span = الامتداد
trace-detail-col-duration = المدّة
trace-detail-root-fallback = (جذر التتبّع)
trace-detail-error-title = خطأ
trace-detail-span-fallback = امتداد
trace-detail-correlated-errors = الأخطاء المترابطة
trace-detail-col-level = المستوى
trace-detail-col-title = العنوان
trace-detail-col-timestamp = الطابع الزمني
trace-detail-span-count = { $count ->
    [zero] لا امتدادات
    [one] امتداد واحد
    [two] امتدادان
    [few] { $count } امتدادات
    [many] { $count } امتدادًا
   *[other] { $count } امتداد
}
