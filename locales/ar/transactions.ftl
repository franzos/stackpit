# واجهة المعاملات: قائمة المعاملات لكل مشروع وصفحة تفاصيل المعاملة (المثيلات).
# تعيد استخدام nav-transactions للعنوان ومسار التنقّل.

# --- لاحقة عنوان الصفحة (عناوين ذات بادئة ديناميكية) ---
transactions-title-suffix = — Stackpit

# --- قائمة المعاملات ---
transactions-time-range = النطاق الزمني
transactions-period-1h = آخر ساعة
transactions-period-24h = آخر 24 ساعة
transactions-period-7d = آخر 7 أيام
transactions-period-14d = آخر 14 يومًا
transactions-period-30d = آخر 30 يومًا
transactions-period-90d = آخر 90 يومًا
transactions-filter-submit = ترشيح
transactions-list-empty = لا توجد معاملات في هذه الفترة.
transactions-col-name = المعاملة
transactions-col-throughput = الإنتاجية
transactions-col-failure = نسبة الفشل %
transactions-col-count = العدد
transactions-col-users = المستخدمون

# --- تفاصيل المعاملة (المثيلات) ---
transactions-detail-op = op:
transactions-detail-empty = لم تُسجَّل مثيلات لهذه المعاملة.
transactions-detail-col-duration = المدّة
transactions-detail-col-status = الحالة
transactions-detail-col-trace = التتبّع
transactions-detail-col-when = الوقت

# --- ترقيم الصفحات (تفاصيل المعاملة) ---
transactions-pagination-label = ترقيم الصفحات
transactions-pagination-prev = « السابق
transactions-pagination-next = التالي »
transactions-detail-count = { $count ->
    [zero] لا مثيلات
    [one] مثيل واحد
    [two] مثيلان
    [few] { $count } مثيلات
    [many] { $count } مثيلًا
   *[other] { $count } مثيل
}
