// data-confirm="message" on a submit button -> window.confirm() before submit.
document.addEventListener('submit', function (e) {
    var btn = e.submitter;
    if (!btn || !btn.dataset || !btn.dataset.confirm) return;
    if (!window.confirm(btn.dataset.confirm)) e.preventDefault();
});
