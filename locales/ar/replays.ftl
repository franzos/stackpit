# واجهة عمليات إعادة التشغيل: قائمة إعادة التشغيل لكل مشروع وصفحة التفاصيل.
# تعيد استخدام nav-replays.

# --- لاحقة عنوان الصفحة ---
replays-title-suffix = — Stackpit

# --- قائمة عمليات إعادة التشغيل ---
replays-list-empty = لم يُعثر على عمليات إعادة تشغيل. ستظهر هنا أحداث إعادة التشغيل.
replays-col-event-id = معرّف الحدث
replays-col-type = النوع
replays-col-release = الإصدار
replays-col-environment = البيئة
replays-col-timestamp = الطابع الزمني

# --- تفاصيل إعادة التشغيل ---
replays-detail-heading = إعادة التشغيل
replays-detail-note = تشغيل التسجيل غير متاح بعد. تُعرض بيانات إعادة التشغيل الخام أدناه.
replays-detail-raw-payload = الحمولة الخام
replays-related-errors = أخطاء في إعادة التشغيل هذه
replays-col-level = المستوى
replays-col-title = العنوان

# --- ترقيم الصفحات ---
replays-pagination-label = ترقيم الصفحات
replays-pagination-prev = « السابق
replays-pagination-next = التالي »
replays-count = { $count ->
    [zero] لا عمليات إعادة تشغيل
    [one] إعادة تشغيل واحدة
    [two] إعادتا تشغيل
    [few] { $count } عمليات إعادة تشغيل
    [many] { $count } عملية إعادة تشغيل
   *[other] { $count } إعادة تشغيل
}
