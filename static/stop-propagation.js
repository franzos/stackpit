// Source links inside <summary> need this so clicking them doesn't toggle <details>.
document.querySelectorAll('[data-stop-propagation]').forEach(function (el) {
    el.addEventListener('click', function (e) { e.stopPropagation(); });
});
