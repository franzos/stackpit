# واجهة الإصدارات: قائمة الإصدارات عبر المشاريع وصفحة صحة الإصدار لكل مشروع.
# تعيد استخدام nav-releases و nav-health.

# --- لاحقة عنوان الصفحة ---
releases-title-suffix = — Stackpit

# --- قائمة الإصدارات ---
releases-list-search-placeholder = ابحث في الإصدارات…
releases-list-search-label = البحث في الإصدارات
releases-list-project-placeholder = معرّف المشروع
releases-list-project-label = ترشيح حسب المشروع
releases-list-period-label = فترة التبنّي
releases-list-period-24h = آخر 24 ساعة
releases-list-period-7d = آخر 7 أيام
releases-list-period-30d = آخر 30 يومًا
releases-filter-submit = ترشيح
releases-list-empty = لا توجد إصدارات بعد. اضبط <code class="text-mono">release</code> في SDK وستظهر هنا بمجرّد وصول الأحداث.
releases-col-version = الإصدار
releases-col-project = المشروع
releases-col-issues = المشكلات
releases-col-events = الأحداث
releases-col-adoption = التبنّي
releases-col-first-seen = أول ظهور
releases-col-last-seen = آخر ظهور

# --- ترقيم الصفحات ---
releases-pagination-label = ترقيم الصفحات
releases-pagination-prev = « السابق
releases-pagination-next = التالي »
releases-count = { $count ->
    [zero] لا إصدارات
    [one] إصدار واحد
    [two] إصداران
    [few] { $count } إصدارات
    [many] { $count } إصدارًا
   *[other] { $count } إصدار
}

# --- صحة الإصدار ---
release-health-title = صحة الإصدار
release-health-heading = صحة الإصدار
release-health-sessions-heading = الجلسات عبر الزمن
release-health-empty = لا تتوفّر بيانات جلسات. ستظهر هنا أحداث الجلسات التي تحمل حقل <code class="text-mono">status</code>.
release-health-col-release = الإصدار
release-health-col-sessions = الجلسات
release-health-col-ok = سليمة
release-health-col-crashed = متعطّلة
release-health-col-errored = فيها أخطاء
release-health-col-crash-free-sessions = جلسات خالية من الأعطال
release-health-col-crash-free-users = مستخدمون بلا أعطال
release-health-subtitle = نتائج الجلسات هي إشارات صحة تُبلّغ عنها SDK، وليست أحداث أخطاء. انقر على إصدار لعرض مشكلاته.
release-health-crashed-title = عرض مشكلات هذا الإصدار
release-health-errored-title = عرض مشكلات هذا الإصدار

# --- تفاصيل الإصدار (لكل نسخة) ---
release-detail-sessions-heading = صحة الجلسات
release-detail-sessions-note = نتائج الجلسات التي تُبلّغ عنها SDK (سليمة / فيها أخطاء / متعطّلة). هذه إشارات صحة، وليست أحداث أخطاء فردية.
release-detail-no-health = لا توجد بيانات جلسات لهذا الإصدار.
release-detail-issues-heading = مشكلات هذا الإصدار
release-detail-issues-note = مجموعات أخطاء مميّزة شوهدت لأول أو آخر مرة مع هذا الإصدار.
release-detail-no-issues = لا توجد مشكلات مسجّلة لهذا الإصدار.
release-health-na = غير متاح
