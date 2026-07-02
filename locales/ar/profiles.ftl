# واجهة ملفات التعريف: قائمة ملفات التعريف لكل مشروع وصفحة تفاصيل ملف التعريف.
# تعيد استخدام nav-profiles.

# --- لاحقة عنوان الصفحة ---
profiles-title-suffix = — Stackpit

# --- قائمة ملفات التعريف ---
profiles-list-empty = لم يُعثر على ملفات تعريف. ستظهر هنا أحداث ملفات التعريف التي تحمل <code class="text-mono">item_type = "profile"</code>.
profiles-col-event-id = معرّف الحدث
profiles-col-transaction = المعاملة
profiles-col-platform = المنصّة
profiles-col-release = الإصدار
profiles-col-environment = البيئة
profiles-col-timestamp = الطابع الزمني

# --- تفاصيل ملف التعريف ---
profiles-detail-heading = ملف التعريف
profiles-detail-raw-payload = الحمولة الخام

# --- ترقيم الصفحات ---
profiles-pagination-label = ترقيم الصفحات
profiles-pagination-prev = « السابق
profiles-pagination-next = التالي »
profiles-count = { $count ->
    [zero] لا ملفات تعريف
    [one] ملف تعريف واحد
    [two] ملفا تعريف
    [few] { $count } ملفات تعريف
    [many] { $count } ملف تعريف
   *[other] { $count } ملف تعريف
}
