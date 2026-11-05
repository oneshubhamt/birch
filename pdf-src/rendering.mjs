import * as pdfjsLib from 'rustnote-pdfjs-display';
import {EventBus, PDFPageView} from 'rustnote-pdfjs-viewer';

const {AnnotationMode} = pdfjsLib;

export const pdfjsPageEventBus = new EventBus();
export const pdfjsPageL10n = Object.freeze({
  async get(_id, _args = null, fallback = '') { return fallback || ''; },
  async translate() {},
  pause() {},
  resume() {},
  getDirection() { return 'ltr'; },
});
export const pdfjsPageLayerProperties = Object.freeze({
  annotationEditorUIManager: null,
  annotationStorage: null,
  downloadManager: null,
  enableComment: false,
  enableScripting: false,
  fieldObjectsPromise: null,
  findController: null,
  hasJSActionsPromise: null,
  linkService: null,
});
export const PDFJS_TEXT_LAYER_DISABLED = 0;

// Keep Mozilla's page/detail canvas lifecycle while preserving Rustnote's
// previous raster semantics: PDF annotations are painted into the canvas, but
// PDF.js does not build a second interactive annotation DOM on top of Rustnote's
// own annotation/search/text layers.
export class RustnotePdfPageView extends PDFPageView {
  _getRenderingContext(canvas, transform, recordOperations, recordImages) {
    return {
      ...super._getRenderingContext(canvas, transform, recordOperations, recordImages),
      annotationMode: AnnotationMode.ENABLE,
    };
  }
}
