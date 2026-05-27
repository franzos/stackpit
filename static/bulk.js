// Bulk-select bar: count badge + select-all checkbox wired to row checkboxes.
(function () {
    var bar = document.querySelector('.bulk-bar');
    if (!bar) return;
    var form = bar.closest('form');
    var boxes = form ? form.querySelectorAll('input[type="checkbox"][name="ids"]') : [];
    if (!boxes.length) return;
    var cnt = document.createElement('span');
    cnt.textContent = '0 selected';
    cnt.style.marginRight = '0.25rem';
    bar.insertBefore(cnt, bar.firstChild);
    bar.style.display = 'none';
    var allTh = form.querySelector('th.col-check');
    if (allTh && !allTh.querySelector('input')) {
        var a = document.createElement('input');
        a.type = 'checkbox';
        a.setAttribute('aria-label', 'Select all');
        allTh.appendChild(a);
        a.addEventListener('change', function () {
            boxes.forEach(function (cb) { cb.checked = a.checked; });
            upd();
        });
    }
    function upd() {
        var n = form.querySelectorAll('input[name="ids"]:checked').length;
        cnt.textContent = n + ' selected';
        bar.style.display = n ? 'flex' : 'none';
    }
    boxes.forEach(function (cb) { cb.addEventListener('change', upd); });
})();
