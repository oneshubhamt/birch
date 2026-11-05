/* Rustnote PDF bootstrap — conservative syntax for older WebKit parsers. */
(function () {
  'use strict';

  var currentScript = document.currentScript;
  var source = currentScript && currentScript.src ? currentScript.src : '';
  var params;
  try {
    params = new URL(source, window.location.href).searchParams;
  } catch (_) {
    params = new URLSearchParams('');
  }

  var attachmentId = params.get('attachment') || '';
  var readOnly = params.get('readonly') === '1';
  var reported = false;

  function describe(error) {
    if (error == null) return 'Unknown PDF module startup error';
    var name = error && error.name ? String(error.name) : '';
    var message = error && error.message ? String(error.message) : String(error);
    var text = name && message.indexOf(name + ':') !== 0 ? name + ': ' + message : message;
    var stack = error && error.stack ? String(error.stack).split('\n').slice(0, 3).join(' | ') : '';
    return (stack && stack.indexOf(message) === -1 ? text + ' | ' + stack : text).slice(0, 700);
  }

  function report(error) {
    if (reported) return;
    reported = true;
    try {
      window.parent && window.parent.postMessage({
        type: 'rustnote-pdf-viewer',
        attachmentId: attachmentId,
        stage: 'error',
        detail: 'PDF.js module startup: ' + describe(error),
      }, '*');
    } catch (_) {
      // Startup reporting must never create a second failure.
    }
  }

  window.addEventListener('error', function (event) {
    report(event && (event.error || event.message));
  });
  window.addEventListener('unhandledrejection', function (event) {
    report(event && event.reason);
  });

  var query = 'attachment=' + encodeURIComponent(attachmentId)
    + (readOnly ? '&readonly=1' : '');

  // Runtime selection intentionally sits above Rustnote's shared PDF
  // transport/render architecture. Only the known-good legacy profile is
  // enabled today; a future capability gate can select a modern profile here
  // without duplicating the viewer/scheduler/virtualization implementation.
  var runtimeProfile = 'legacy';
  var moduleUrl = '/rustnotepdf/runtime/' + runtimeProfile + '/viewer.mjs?' + query;
  import(moduleUrl).catch(report);
})();
