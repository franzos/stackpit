# واجهة المقاييس: قائمة المقاييس لكل مشروع وصفحة تفاصيل سلسلة المقياس.
# تعيد استخدام nav-metrics.

# --- لاحقة عنوان الصفحة ---
metrics-title-suffix = — Stackpit

# --- قائمة المقاييس ---
metrics-list-empty = لم يُعثر على مقاييس. ستظهر أحداث المقاييس هنا بمجرّد استلامها.
metrics-col-mri = MRI
metrics-col-type = النوع
metrics-col-data-points = نقاط البيانات
metrics-col-first-seen = أول ظهور
metrics-col-last-seen = آخر ظهور

# --- ترقيم الصفحات ---
metrics-pagination-label = ترقيم الصفحات
metrics-pagination-prev = « السابق
metrics-pagination-next = التالي »
metrics-count = { $count ->
    [zero] لا مقاييس
    [one] مقياس واحد
    [two] مقياسان
    [few] { $count } مقاييس
    [many] { $count } مقياسًا
   *[other] { $count } مقياس
}

# --- تفاصيل المقياس (تجميعات بالساعة) ---
metrics-detail-empty = لا توجد نقاط بيانات في النطاق الزمني المحدّد.
metrics-detail-col-time = الوقت (تجميع بالساعة)
metrics-detail-col-count = العدد
metrics-detail-col-sum = المجموع
metrics-detail-col-min = الأدنى
metrics-detail-col-max = الأقصى
metrics-detail-col-avg = المتوسّط
