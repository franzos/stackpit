# واجهة المراقبات: قائمة المراقبات (تسجيلات وصول cron) لكل مشروع وصفحة تفاصيل
# المراقبة. تعيد استخدام nav-monitors.

# --- لاحقة عنوان الصفحة ---
monitors-title-suffix = — Stackpit

# --- قائمة المراقبات ---
monitors-list-empty = لم يُعثر على مراقبات. ستظهر هنا أحداث تسجيل الوصول التي تحمل <code class="text-mono">monitor_slug</code>.
monitors-col-slug = المُعرّف اللطيف
monitors-col-last-status = آخر حالة
monitors-col-last-checkin = آخر تسجيل وصول
monitors-col-count = العدد

# --- تفاصيل المراقبة ---
monitors-detail-title-prefix = المراقبة
monitors-detail-subtitle = تسجيلات وصول المراقبة.
monitors-detail-empty = لم يُعثر على تسجيلات وصول لهذه المراقبة.
monitors-detail-select-checkin = تحديد تسجيل وصول
monitors-detail-confirm-delete-selected = هل تريد حذف تسجيلات الوصول المحدّدة؟
monitors-detail-delete = حذف
monitors-detail-col-title = العنوان
monitors-detail-col-level = المستوى
monitors-detail-col-environment = البيئة
monitors-detail-col-time = الوقت
monitors-detail-untitled = (بلا عنوان)
monitors-detail-confirm-delete-all = { $count ->
    [zero] هل تريد حذف جميع تسجيلات الوصول؟
    [one] هل تريد حذف تسجيل الوصول الواحد؟
    [two] هل تريد حذف تسجيلَي الوصول؟
    [few] هل تريد حذف جميع تسجيلات الوصول ({ $count })؟
    [many] هل تريد حذف جميع تسجيلات الوصول ({ $count })؟
   *[other] هل تريد حذف جميع تسجيلات الوصول ({ $count })؟
}
monitors-detail-delete-all = { $count ->
    [zero] حذف الكل ({ $count })
    [one] حذف الكل ({ $count })
    [two] حذف الكل ({ $count })
    [few] حذف الكل ({ $count })
    [many] حذف الكل ({ $count })
   *[other] حذف الكل ({ $count })
}

# --- ترقيم الصفحات ---
monitors-pagination-label = ترقيم الصفحات
monitors-pagination-prev = « السابق
monitors-pagination-next = التالي »
monitors-detail-count = { $count ->
    [zero] لا تسجيلات وصول
    [one] تسجيل وصول واحد
    [two] تسجيلا وصول
    [few] { $count } تسجيلات وصول
    [many] { $count } تسجيل وصول
   *[other] { $count } تسجيل وصول
}
