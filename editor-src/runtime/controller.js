import {toggleMark} from "prosemirror-commands";
import {closeHistory, redo, undo} from "prosemirror-history";
import {Fragment} from "prosemirror-model";
import {EditorState, NodeSelection, TextSelection} from "prosemirror-state";
import {addColumnAfter, addColumnBefore, addRowAfter, addRowBefore, cellAround, deleteColumn, deleteRow} from "prosemirror-tables";
import {EditorView} from "prosemirror-view";
import {canonicalAttachmentKind, discardStagedImageImports, isImageImportFile, mediaTimeLabelStatic, stageImageImportFiles} from "../attachments/import.js";
import {AttachmentView, attachmentNodeAttrs} from "../attachments/view.js";
import {ATTACHMENT_CONTEXT_MENU_ID, CODE_CONTEXT_MENU_ID, DEFAULT_AUTO_FIRST_LINE_INDENT, EDITOR_SYNC_DELAY_MS, EDITOR_SYNC_MAX_DELAY_MS, FLOATING_TOOLBAR_ID, HIGHLIGHT_COLORS, INSERT_COMMANDS, INSERT_MENU_ID, INSERT_TRIGGER_ID, LINE_SPACINGS, LINK_CONTEXT_MENU_ID, MAX_TRANSIENT_IMAGE_IMPORT_BYTES, SCIENTIFIC_COMPOSER_DESCRIPTION_ID, SCIENTIFIC_COMPOSER_TITLE_ID, SCIENTIFIC_PREVIEW_DELAY_MS, SCIENTIFIC_SYMBOL_GROUPS, SCIENTIFIC_TEMPLATE_GROUPS, SCIENTIFIC_TOOLS_DESCRIPTION_ID, SCIENTIFIC_TOOLS_TITLE_ID, SLASH_COMMANDS, SURFACE_GAP, TABLE_ACTION_MENU_ID, TABLE_COLUMN_CONTROL_ID, TABLE_GRID_SIZE, TABLE_LEVEL_CONTROL_ID, TABLE_ROW_CONTROL_ID, TABLE_TOOLBAR_ID, VIEWPORT_MARGIN, findHighlightKey, preservedSelectionKey} from "../config.js";
import {configureFloatingSurface, enabledCommandMenuItems, focusCommandMenuItem, handleCommandMenuKeyDown} from "../ui/menu.js";
import {GUIDED_STRUCTURE_LABELS, ScientificMathView, TaskItemView, activeScientificInsertionTarget, ancestorDepth, autoLinkMatch, clamp, clampDocumentPosition, cloneGuidedData, continueOrderedNumbering, currentTableContext, defaultGuidedScientificData, displayScientificToInlineTransaction, documentIsEmpty, editorEnvelope, guidedScientificEligibility, guidedScientificSource, guidedScientificUnavailableMessage, inlineScientificToDisplayTransaction, insertGuidedValueAtPath, insertIntoTextarea, normalizeAtomicBoundaryDocument, plugins, renderGuidedScientificForm, renderScientificSource, resolveGuidedStructure, restartOrderedNumbering, scientificBoundarySelection, scientificComposerInsertionMode, scientificNodeContext, scientificNodeMarkdown, scientificNodeText, scientificTemplateCaretOffset, scientificToolGroupsPanel, selectedBlockStyle, selectedLinkContext, selectedListType, selectedMarkAttribute, selectedTextblocks, selectionIsInsideNodeType, selectionUsesMark, syncTaskCheckboxInteractionState, tableDomContext, tableHasHeaderRow, updateToolbarState} from "../editor/core.js";
import {liftSelectedListItems, sinkSelectedListItems} from "../editor/list_system.js";
import {buildFindDocumentIndex, captureSelection, clearParagraphLayout, clearTypography, adjustParagraphIndent, commandForBlock, commandForList, commandMatchesQuery, copyPlainText, copyScientificClipboard, deleteActiveTable, findMatches, focusWithoutScroll, insertHorizontalRule, insertTable, newDurableRequestId, normalizedFindQuery, restoreSelection, selectionIsInsideTable, setActiveTableColumnWidths, setBulletListMarker, setFontFamily, setFontSize, setHighlightColor, setLineSpacing, setParagraphLayoutAttribute, setTextAlignment, setOrderedListStyle, setTextColor, toggleActiveTableHeaderRow, toggleDropCap, toggleHighlightColor, writeBridge} from "../commands/index.js";
import {attachmentStatus, emitAttachmentCommand, emitNoteLinkCommand} from "../bridge/events.js";
import {MEDIA_DEEP_LINK_STORAGE_KEY, defaultFontSizeForBlockStyle, mediaDeepLinkUri, normalizeSafeHref, parseEnvelope, pruneMediaResumeStorage, schema, validAttachmentId, validMediaLinkAttrs, validNoteId} from "../document/schema.js";
import {specDomain, specLimit, truncateUtf8} from "../document/spec.js";
import {activeEditorController, clearEditorRuntimeError, setEditorRuntimeError} from "./state.js";


function preferredSpellcheckLanguage() {
  const documentLanguage = typeof globalThis.document?.documentElement?.lang === 'string'
    ? globalThis.document.documentElement.lang.trim()
    : '';
  const navigatorLanguage = typeof globalThis.navigator?.language === 'string'
    ? globalThis.navigator.language.trim()
    : '';
  return documentLanguage || navigatorLanguage;
}

function nativeSpellcheckAttributes(enabled) {
  const attributes = {
    spellcheck: String(Boolean(enabled)),
    'data-spellcheck-enabled': String(Boolean(enabled)),
  };
  const language = preferredSpellcheckLanguage();
  if (language) attributes.lang = language;
  return attributes;
}

class Controller {
  constructor(root, doc, noteId = '', generation = 0, editable = true, imageImportStageUrl = '', attachmentRegistry = [], noteLinkTargets = [], attachmentImportIntents = null, autoIndentEnabled = false, autoFirstLineIndent = DEFAULT_AUTO_FIRST_LINE_INDENT, defaultLineSpacing = null, spellcheckEnabled = true) {
    this.root = root;
    this.noteId = validNoteId(noteId) ? noteId : '';
    this.imageImportStageUrl = String(imageImportStageUrl || '');
    this.attachmentRegistry = new Map();
    this.attachmentViews = new Set();
    this.mediaCompatibilityStates = new Map();
    this.mediaCompatibilityProfiles = new Map();
    this.mediaCompatibilityFailedProfiles = new Map();
    pruneMediaResumeStorage();
    this.pendingImageImports = new Map();
    this.nextImageImportRequest = 1;
    this.pendingPdfExports = new Map();
    this.focusMode = 'none';
    this.focusedPdfView = null;
    this.pdfFocusReturnMode = 'none';
    this.focusedImageView = null;
    this.imageFocusReturnMode = 'none';
    this.focusedImageEditView = null;
    this.imageEditReturnMode = 'none';
    this.imageEditImageReturnMode = 'none';
    this.pendingImageEdits = new Map();
    this.nextImageEditRequest = 1;
    this.pendingNoteLinkPickers = new Map();
    this.nextNoteLinkPickerRequest = 1;
    this.noteLinkRegistry = new Map();
    this.setAttachmentRegistry(attachmentRegistry);
    this.setNoteLinkRegistry(noteLinkTargets);
    this.serializeTimer = null;
    this.serializeIdleHandle = null;
    this.dirtySince = null;
    this.documentRevision = 0;
    this.lastSynchronizedRevision = -1;
    this.lastSynchronizedGeneration = -1;
    this.cachedEnvelopeRevision = -1;
    this.cachedEnvelopeGeneration = -1;
    this.cachedEnvelope = '';
    this.findIndexRevision = -1;
    this.findDocumentIndex = [];
    this.findMatchCache = new Map();
    this.discreteActionDepth = 0;
    this.historyBoundaryPending = false;
    this.generation = Number.isSafeInteger(Number(generation))
      ? Math.max(0, Number(generation))
      : 0;
    this.cachedCanonicalRevision = -1;
    this.cachedCanonicalState = '';
    this.feedbackTimer = null;
    this.feedbackElement = null;
    this.destroyed = false;
    this.editable = Boolean(editable);
    this.autoIndentEnabled = Boolean(autoIndentEnabled);
    this.autoFirstLineIndent = this.normalizedAutoFirstLineIndent(autoFirstLineIndent);
    this.defaultLineSpacing = LINE_SPACINGS.includes(String(defaultLineSpacing || '')) ? String(defaultLineSpacing) : null;
    this.spellcheckEnabled = spellcheckEnabled !== false;
    this.initialising = true;
    this.preservedSelection = null;
    this.preservedLinkSelection = null;
    this.floatingToolbar = null;
    this.linkContextMenu = null;
    this.linkContext = null;
    this.linkContextReturnFocus = null;
    this.attachmentContextMenu = null;
    this.attachmentContextView = null;
    this.attachmentContextReturnFocus = null;
    this.codeContextMenu = null;
    this.codeContext = null;
    this.codeContextReturnFocus = null;
    this.insertMenu = null;
    this.insertMenuSource = null;
    this.insertMenuQuery = '';
    this.insertMenuCommands = [];
    this.insertMenuIndex = 0;
    this.slashContext = null;
    this.dismissedSlashPosition = null;
    this.suppressSlashMenu = false;
    this.tableGridOpen = false;
    this.tableGridRows = 3;
    this.tableGridColumns = 3;
    this.tableGridHeader = true;
    this.scientificComposerKind = null;
    this.scientificComposerDisplay = false;
    this.scientificComposerInitialValue = '';
    this.scientificComposerOriginalText = '';
    this.scientificComposerSmartPaste = false;
    this.scientificComposerSelectionStart = null;
    this.scientificComposerSelectionEnd = null;
    this.scientificComposerViewMode = 'guided';
    this.scientificComposerGuidedKind = 'expression';
    this.scientificComposerGuidedData = defaultGuidedScientificData('expression');
    this.scientificComposerTarget = 'new';
    this.scientificComposerReturn = null;
    this.scientificToolsOpen = false;
    this.scientificToolsAnchor = null;
    this.scientificToolsFocusIndex = 0;
    this.scientificPreviewTimer = null;
    this.scientificPreviewFrame = null;
    this.scientificPreviewGeneration = 0;
    this.tableToolbar = null;
    this.tableLevelControl = null;
    this.tableRowControl = null;
    this.tableColumnControl = null;
    this.tableActionMenu = null;
    this.tableActionMenuKind = null;
    this.tableActionMenuAnchor = null;
    this.tableActionMenuSelection = null;
    this.tableGeometryFrame = null;
    this.lastHighlightColor = 'yellow';
    this.view = new EditorView(root, {
      state: EditorState.create({doc, schema, plugins: plugins()}),
      editable: () => this.editable,
      attributes: () => ({
        role: 'textbox',
        'aria-multiline': 'true',
        'aria-label': 'Note body',
        'aria-readonly': String(!this.editable),
        ...nativeSpellcheckAttributes(this.spellcheckEnabled),
        'data-placeholder': 'Start writing',
      }),
      nodeViews: {
        task_item: (node, view, getPos) => new TaskItemView(node, view, getPos),
        attachment: (node, view, getPos) => new AttachmentView(node, view, getPos, this),
        math_inline: (node, view, getPos) => new ScientificMathView(node, view, getPos),
        math_display: (node, view, getPos) => new ScientificMathView(node, view, getPos),
      },
      dispatchTransaction: transaction => {
        if (!this.editable && transaction.docChanged) {
          return;
        }
        if (transaction.docChanged && this.historyBoundaryPending) {
          transaction = closeHistory(transaction);
          this.historyBoundaryPending = false;
        }
        if (transaction.docChanged && this.pendingImageImports.size > 0) {
          for (const pending of this.pendingImageImports.values()) {
            if (pending.mode === 'insert' && pending.bookmark) {
              pending.bookmark = pending.bookmark.map(transaction.mapping);
            }
          }
        }
        if (transaction.docChanged && this.pendingNoteLinkPickers.size > 0) {
          for (const pending of this.pendingNoteLinkPickers.values()) {
            if (pending.bookmark) {
              pending.bookmark = pending.bookmark.map(transaction.mapping);
            }
          }
        }
        const nextState = this.view.state.apply(transaction);
        this.view.updateState(nextState);
        this.preservedSelection = captureSelection(nextState.selection);
        this.refreshSurface();
        if (transaction.docChanged) {
          this.documentRevision += 1;
          this.scheduleSerialization();
        }
      },
    });
    this.view.dom.classList.add('prosemirror-root');
    this.applyNativeSpellcheckState(false);
    this.onEditorClickBoundary = event => {
      // Dioxus delegates bubbling click events at its application root and resolves
      // the target's mounted id only after inner handlers have run. ProseMirror
      // owns this subtree and may replace/remove clicked DOM synchronously, which
      // can leave Dioxus with a detached target and a null element id. Keep clicks
      // inside the editor boundary after ProseMirror/NodeView handlers have run.
      event.stopPropagation();
    };
    this.root.addEventListener('click', this.onEditorClickBoundary);
    this.root.dataset.runtimeReady = 'true';
    this.root.dataset.editable = String(this.editable);
    this.createFloatingToolbar();
    this.createLinkContextMenu();
    this.createAttachmentContextMenu();
    this.createCodeContextMenu();
    this.createInsertMenu();
    this.createTableSurfaces();
    this.onDocumentPointerDown = event => {
      if (this.destroyed || !(event.target instanceof Node)) {
        return;
      }
      const trigger = document.getElementById(INSERT_TRIGGER_ID);
      if (
        this.insertMenuSource !== null
        && !this.insertMenu?.contains(event.target)
        && !trigger?.contains(event.target)
      ) {
        this.closeInsertMenu(false);
      }
      if (
        this.linkContextMenu
        && !this.linkContextMenu.hidden
        && !this.linkContextMenu.contains(event.target)
      ) {
        this.closeLinkContextMenu();
      }
      if (
        this.attachmentContextMenu
        && !this.attachmentContextMenu.hidden
        && !this.attachmentContextMenu.contains(event.target)
      ) {
        this.closeAttachmentContextMenu();
      }
      if (
        this.codeContextMenu
        && !this.codeContextMenu.hidden
        && !this.codeContextMenu.contains(event.target)
      ) {
        this.closeCodeContextMenu();
      }
      if (
        this.tableActionMenuKind !== null
        && !this.tableActionMenu?.contains(event.target)
        && !this.tableLevelControl?.contains(event.target)
        && !this.tableRowControl?.contains(event.target)
        && !this.tableColumnControl?.contains(event.target)
        && !this.tableToolbar?.contains(event.target)
      ) {
        this.closeTableActionMenu();
      }
      for (const attachmentView of this.attachmentViews) {
        if (attachmentView.node?.attrs?.kind !== 'image') continue;
        if (!attachmentView.controls?.contains(event.target)) attachmentView.closeImageMenus();
      }
    };
    this.onDocumentClick = event => {
      if (this.destroyed || !(event.target instanceof Element)) return;
      const mediaLink = event.target.closest('.prosemirror-media-link[data-rustnote-media-link]');
      if (mediaLink instanceof HTMLElement && this.root.contains(mediaLink)) {
        event.preventDefault();
        event.stopPropagation();
        const attrs = {
          noteId: mediaLink.dataset.noteId || '',
          attachmentId: mediaLink.dataset.attachmentId || '',
          startMs: Number(mediaLink.dataset.mediaStartMs),
          endMs: mediaLink.dataset.mediaEndMs ? Number(mediaLink.dataset.mediaEndMs) : null,
        };
        this.queueMediaDeepLink(attrs);
        return;
      }
      const noteLink = event.target.closest('.prosemirror-note-link[data-note-id]');
      if (noteLink instanceof HTMLElement && this.root.contains(noteLink)) {
        event.preventDefault();
        event.stopPropagation();
        const noteId = noteLink.dataset.noteId;
        if (validNoteId(noteId)) {
          emitNoteLinkCommand({kind: 'open_note', note_id: noteId});
        }
        return;
      }
      const link = event.target.closest('a.prosemirror-link');
      if (link instanceof HTMLAnchorElement && this.root.contains(link)) {
        event.preventDefault();
        event.stopPropagation();
        this.openLinkContextMenu(link);
        return;
      }
      const copyButton = event.target.closest('[data-live-editor-scientific-copy]');
      if (!(copyButton instanceof HTMLElement)) return;
      const mode = copyButton.dataset.liveEditorScientificCopy;
      if (mode !== 'source' && mode !== 'delimited') return;
      event.preventDefault();
      event.stopPropagation();
      this.copyScientific(mode);
    };
    this.onDocumentContextMenu = event => {
      if (this.destroyed || !(event.target instanceof Element)) return;
      const link = event.target.closest('a.prosemirror-link');
      if (link instanceof HTMLAnchorElement && this.root.contains(link)) {
        event.preventDefault();
        event.stopPropagation();
        this.openLinkContextMenu(link, {x: event.clientX, y: event.clientY});
        return;
      }
      const codeElement = event.target.closest('pre.prosemirror-code');
      if (codeElement instanceof HTMLElement && this.root.contains(codeElement)) {
        event.preventDefault();
        event.stopPropagation();
        this.openCodeContextMenu(codeElement, {x: event.clientX, y: event.clientY});
        return;
      }
      const attachmentElement = event.target.closest('.prosemirror-attachment-node');
      if (attachmentElement instanceof HTMLElement && this.root.contains(attachmentElement)) {
        const nativeControl = event.target.closest('input, textarea, select, [contenteditable="true"]');
        if (nativeControl && attachmentElement.contains(nativeControl) && nativeControl !== attachmentElement) return;
        const attachmentView = this.attachmentViewForElement(attachmentElement);
        if (attachmentView) {
          event.preventDefault();
          event.stopPropagation();
          this.openAttachmentContextMenu(attachmentView, {x: event.clientX, y: event.clientY});
          return;
        }
      }
      const tableCell = event.target.closest('td, th');
      if (tableCell instanceof HTMLElement && this.root.contains(tableCell) && this.activateTableCellElement(tableCell)) {
        event.preventDefault();
        event.stopPropagation();
        this.openTableActionMenu('table', tableCell, {x: event.clientX, y: event.clientY});
      }
    };
    this.onDocumentKeyDown = event => {
      if (this.destroyed || event.isComposing) {
        return;
      }
      if (
        this.linkContextMenu
        && !this.linkContextMenu.hidden
        && handleCommandMenuKeyDown(event, this.linkContextMenu, () => this.closeLinkContextMenu(true))
      ) return;
      if (
        this.attachmentContextMenu
        && !this.attachmentContextMenu.hidden
        && handleCommandMenuKeyDown(event, this.attachmentContextMenu, () => this.closeAttachmentContextMenu(true))
      ) return;
      if (
        this.codeContextMenu
        && !this.codeContextMenu.hidden
        && handleCommandMenuKeyDown(event, this.codeContextMenu, () => this.closeCodeContextMenu(true))
      ) return;
      if (
        this.tableActionMenuKind !== null
        && handleCommandMenuKeyDown(event, this.tableActionMenu, () => this.closeTableActionMenu(true))
      ) return;
      const keyboardContextRequest = event.key === 'ContextMenu'
        || (event.key === 'F10' && event.shiftKey && !event.metaKey && !event.ctrlKey && !event.altKey);
      if (keyboardContextRequest && event.target instanceof Element) {
        const link = event.target.closest('a.prosemirror-link');
        if (link instanceof HTMLAnchorElement && this.root.contains(link)) {
          event.preventDefault();
          event.stopPropagation();
          this.openLinkContextMenu(link);
          return;
        }
        const attachmentElement = event.target.closest('.prosemirror-attachment-node');
        const attachmentView = attachmentElement instanceof HTMLElement && this.root.contains(attachmentElement)
          ? this.attachmentViewForElement(attachmentElement)
          : null;
        if (attachmentView) {
          event.preventDefault();
          event.stopPropagation();
          this.openAttachmentContextMenu(attachmentView);
          return;
        }
        const codeContext = this.codeBlockContextForElement(null);
        if (codeContext) {
          event.preventDefault();
          event.stopPropagation();
          this.openCodeContextMenu(codeContext.element);
          return;
        }
        const tableContext = tableDomContext(this.view);
        if (tableContext) {
          event.preventDefault();
          event.stopPropagation();
          this.openTableActionMenu('table', tableContext.cell);
          return;
        }
      }
      if (
        event.key === 'Enter'
        && !event.metaKey
        && !event.ctrlKey
        && !event.altKey
        && event.target instanceof Element
      ) {
        const mediaLink = event.target.closest('.prosemirror-media-link[data-rustnote-media-link]');
        if (mediaLink instanceof HTMLElement && this.root.contains(mediaLink)) {
          const attrs = {
            noteId: mediaLink.dataset.noteId || '',
            attachmentId: mediaLink.dataset.attachmentId || '',
            startMs: Number(mediaLink.dataset.mediaStartMs),
            endMs: mediaLink.dataset.mediaEndMs ? Number(mediaLink.dataset.mediaEndMs) : null,
          };
          if (validMediaLinkAttrs(attrs)) {
            event.preventDefault();
            event.stopPropagation();
            this.queueMediaDeepLink(attrs);
          }
          return;
        }
        const noteLink = event.target.closest('.prosemirror-note-link[data-note-id]');
        if (noteLink instanceof HTMLElement && this.root.contains(noteLink)) {
          const noteId = noteLink.dataset.noteId;
          if (validNoteId(noteId)) {
            event.preventDefault();
            event.stopPropagation();
            emitNoteLinkCommand({kind: 'open_note', note_id: noteId});
          }
          return;
        }
      }
      if (event.key === 'Escape' && this.focusMode === 'image_edit') {
        if (this.focusedImageEditView?.imageEditSaving) return;
        if (event.target instanceof HTMLTextAreaElement && event.target.classList.contains('prosemirror-image-edit-text-editor')) return;
        event.preventDefault();
        event.stopPropagation();
        this.exitImageEditFocus(this.focusedImageEditView, true, false);
        return;
      }
      if (event.key === 'Escape' && this.focusMode === 'image') {
        event.preventDefault();
        event.stopPropagation();
        this.exitFocusMode('image');
        return;
      }
      if (event.key === 'Escape' && this.focusMode === 'pdf') {
        event.preventDefault();
        event.stopPropagation();
        this.exitFocusMode('pdf');
        return;
      }
      if (!this.editable) {
        if (event.key === 'Escape' && this.focusMode === 'editor') {
          event.preventDefault();
          event.stopPropagation();
          this.exitFocusMode('editor');
        }
        return;
      }
      if (
        (event.metaKey || event.ctrlKey)
        && !event.altKey
        && event.key.toLowerCase() === 'k'
        && this.view.dom.contains(event.target)
      ) {
        event.preventDefault();
        event.stopPropagation();
        this.hideFloatingToolbar();
        document.getElementById('editor-link-trigger')?.click();
        return;
      }
      if (
        event.key === 'Enter'
        && !event.metaKey
        && !event.ctrlKey
        && !event.altKey
        && this.view.dom.contains(event.target)
      ) {
        this.linkUrlBeforeCursor();
      }
      if (
        this.tableActionMenuKind !== null
        && handleCommandMenuKeyDown(event, this.tableActionMenu, () => this.closeTableActionMenu(true))
      ) return;
      if (this.insertMenuSource !== null && this.handleInsertMenuKeyDown(event)) {
        return;
      }
      if (event.key === 'Escape' && this.focusMode === 'editor') {
        if (this.hasOpenFocusTransientSurface()) return;
        event.preventDefault();
        event.stopPropagation();
        this.exitFocusMode('editor');
      }
    };
    this.onPageHide = () => {
      if (!this.destroyed) {
        for (const attachmentView of this.attachmentViews) attachmentView.persistMediaPlaybackPosition?.(true);
        this.flush(true);
      }
    };
    this.onVisibilityChange = () => {
      if (!this.destroyed && document.visibilityState === 'hidden') {
        for (const attachmentView of this.attachmentViews) attachmentView.persistMediaPlaybackPosition?.(true);
        this.flush(true);
      }
    };
    this.onViewportChange = () => {
      if (this.destroyed) {
        return;
      }
      this.positionFloatingToolbar();
      this.positionLinkContextMenu();
      this.positionAttachmentContextMenu();
      this.positionCodeContextMenu();
      this.positionInsertMenu();
      this.scheduleTableSurfacePosition();
      if (this.focusMode === 'image' && this.focusedImageView) {
        this.focusedImageView.handleImageFocusViewportChange();
      } else if (this.focusMode === 'image_edit' && this.focusedImageEditView) {
        this.focusedImageEditView.handleImageEditViewportChange();
      }
    };
    this.onDocumentFocusChange = () => {
      window.requestAnimationFrame(() => {
        if (!this.destroyed) {
          updateToolbarState(this.view);
          this.updateFloatingToolbar();
          this.updateTableSurfaces();
        }
      });
    };
    document.addEventListener('pointerdown', this.onDocumentPointerDown, true);
    document.addEventListener('click', this.onDocumentClick, true);
    document.addEventListener('contextmenu', this.onDocumentContextMenu, true);
    document.addEventListener('keydown', this.onDocumentKeyDown, true);
    window.addEventListener('pagehide', this.onPageHide);
    document.addEventListener('visibilitychange', this.onVisibilityChange);
    document.addEventListener('scroll', this.onViewportChange, true);
    window.addEventListener('resize', this.onViewportChange);
    document.addEventListener('focusin', this.onDocumentFocusChange, true);
    document.addEventListener('focusout', this.onDocumentFocusChange, true);
    this.initialising = false;
    this.recoverAttachmentPickerImports(attachmentImportIntents);
    this.refreshSurface();
    window.requestAnimationFrame(() => this.consumePendingMediaDeepLink());
  }

  queueMediaDeepLink(attrs) {
    if (!validMediaLinkAttrs(attrs)) return false;
    if (attrs.noteId === this.noteId) {
      const found = this.seekMediaDeepLink(attrs);
      if (!found) this.showEditorFeedback('Linked media attachment is not available', true);
      return found;
    }
    try {
      sessionStorage.setItem(MEDIA_DEEP_LINK_STORAGE_KEY, JSON.stringify({...attrs, queuedAt: Date.now()}));
    } catch { /* session storage may be unavailable */ }
    return emitNoteLinkCommand({kind: 'open_note', note_id: attrs.noteId});
  }

  consumePendingMediaDeepLink() {
    if (!this.noteId) return false;
    let raw = '';
    try { raw = sessionStorage.getItem(MEDIA_DEEP_LINK_STORAGE_KEY) || ''; } catch { return false; }
    if (!raw) return false;
    let attrs = null;
    try { attrs = JSON.parse(raw); } catch { /* invalid pending target */ }
    const queuedAt = Number(attrs?.queuedAt);
    if (!validMediaLinkAttrs(attrs) || !Number.isFinite(queuedAt) || Date.now() - queuedAt > 120_000) {
      try { sessionStorage.removeItem(MEDIA_DEEP_LINK_STORAGE_KEY); } catch { /* best effort */ }
      return false;
    }
    if (attrs.noteId !== this.noteId) return false;
    try { sessionStorage.removeItem(MEDIA_DEEP_LINK_STORAGE_KEY); } catch { /* best effort */ }
    const found = this.seekMediaDeepLink(attrs);
    if (!found) this.showEditorFeedback('Linked media attachment is not available', true);
    return found;
  }

  seekMediaDeepLink(attrs) {
    if (!validMediaLinkAttrs(attrs)) return false;
    const attachmentView = [...this.attachmentViews].find(candidate => (
      candidate?.node?.attrs?.attachmentId === attrs.attachmentId
      && (candidate.node.attrs.kind === 'audio' || candidate.node.attrs.kind === 'video')
    ));
    if (!attachmentView) return false;
    attachmentView.seekToMediaLink(attrs.startMs, attrs.endMs);
    attachmentView.dom.scrollIntoView({block: 'center', behavior: 'smooth'});
    return true;
  }

  async copyMediaDeepLink(attachmentView, startMs, endMs = null, label = '') {
    if (!this.noteId || !attachmentView?.node) return false;
    const attrs = {
      noteId: this.noteId,
      attachmentId: attachmentView.node.attrs.attachmentId,
      startMs: Math.max(0, Math.round(Number(startMs) || 0)),
      endMs: Number.isFinite(Number(endMs)) ? Math.max(0, Math.round(Number(endMs))) : null,
    };
    if (!validMediaLinkAttrs(attrs)) attrs.endMs = null;
    if (!validMediaLinkAttrs(attrs)) return false;
    const uri = mediaDeepLinkUri(attrs);
    const rangeLabel = Number.isSafeInteger(attrs.endMs)
      ? `${mediaTimeLabelStatic(attrs.startMs / 1000)}–${mediaTimeLabelStatic(attrs.endMs / 1000)}`
      : mediaTimeLabelStatic(attrs.startMs / 1000);
    const textLabel = String(label || '').trim() || `${attachmentView.node.attrs.caption || attachmentView.node.attrs.displayName} · ${rangeLabel}`;
    try {
      if (navigator.clipboard?.write && typeof ClipboardItem === 'function') {
        const span = document.createElement('span');
        span.className = 'prosemirror-note-link prosemirror-media-link';
        span.dataset.rustnoteMediaLink = 'true';
        span.dataset.noteId = attrs.noteId;
        span.dataset.attachmentId = attrs.attachmentId;
        span.dataset.mediaStartMs = String(attrs.startMs);
        if (Number.isSafeInteger(attrs.endMs)) span.dataset.mediaEndMs = String(attrs.endMs);
        span.textContent = textLabel;
        const item = new ClipboardItem({
          'text/plain': new Blob([uri], {type: 'text/plain'}),
          'text/html': new Blob([span.outerHTML], {type: 'text/html'}),
        });
        await navigator.clipboard.write([item]);
        return true;
      }
    } catch { /* fall through to plain text */ }
    return copyPlainText(uri);
  }

  runDiscreteUserAction(action) {
    const outermost = this.discreteActionDepth === 0;
    if (outermost) {
      this.historyBoundaryPending = true;
    }
    this.discreteActionDepth += 1;
    try {
      return action();
    } finally {
      this.discreteActionDepth -= 1;
      if (outermost) {
        const documentChanged = !this.historyBoundaryPending;
        this.historyBoundaryPending = false;
        if (documentChanged && !this.destroyed) {
          this.view.dispatch(closeHistory(this.view.state.tr).setMeta('addToHistory', false));
        }
      }
    }
  }

  refreshSurface() {
    this.root.dataset.empty = documentIsEmpty(this.view.state.doc) ? 'true' : 'false';
    this.view.dom.dataset.empty = this.root.dataset.empty;
    updateToolbarState(this.view);
    this.refreshNoteLinkStates();
    for (const table of this.view.dom.querySelectorAll('table')) {
      if (table instanceof HTMLElement) {
        table.dataset.contextLongPress = 'true';
        table.setAttribute('aria-keyshortcuts', 'Shift+F10');
      }
    }
    if (!this.editable) {
      this.hideFloatingToolbar();
      this.closeInsertMenu(false);
      this.hideTableSurfaces();
      return;
    }
    this.updateFloatingToolbar();
    this.updateSlashMenu();
    this.updateTableSurfaces();
  }


  createLinkContextMenu() {
    const menu = document.createElement('div');
    menu.id = LINK_CONTEXT_MENU_ID;
    menu.className = 'prosemirror-link-context-menu';
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Link actions');
    menu.hidden = true;
    menu.addEventListener('keydown', event => {
      handleCommandMenuKeyDown(event, menu, () => this.closeLinkContextMenu(true));
    });
    const actions = [
      ['Open link', () => this.openCurrentLink(), false],
      ['Edit link', () => this.editCurrentLink(), true],
      ['Copy link', () => this.copyCurrentLink(), false],
      ['Remove link', () => this.removeCurrentLink(), true],
    ];
    for (const [label, action, requiresEditable] of actions) {
      const button = document.createElement('button');
      button.type = 'button';
      button.setAttribute('role', 'menuitem');
      button.dataset.requiresEditable = requiresEditable ? 'true' : 'false';
      button.textContent = label;
      button.addEventListener('pointerdown', event => {
        event.preventDefault();
        event.stopPropagation();
      });
      button.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        action();
      });
      menu.appendChild(button);
    }
    document.body.appendChild(menu);
    this.linkContextMenu = menu;
    this.refreshLinkContextMenuInteractionState();
  }

  refreshLinkContextMenuInteractionState() {
    if (!this.linkContextMenu) return;
    this.linkContextMenu.querySelectorAll('button[data-requires-editable="true"]').forEach(button => {
      button.disabled = !this.editable;
      button.setAttribute('aria-disabled', this.editable ? 'false' : 'true');
    });
  }

  linkContextForAnchor(anchor) {
    const href = normalizeSafeHref(anchor.getAttribute('href') || '');
    if (!href) return null;
    try {
      const from = this.view.posAtDOM(anchor, 0);
      const to = this.view.posAtDOM(anchor, anchor.childNodes.length);
      if (!Number.isFinite(from) || !Number.isFinite(to) || from >= to) return null;
      return {from, to, href, label: this.view.state.doc.textBetween(from, to, ' '), anchor};
    } catch {
      return null;
    }
  }

  openLinkContextMenu(anchor, point = null) {
    this.closeAttachmentContextMenu();
    this.closeCodeContextMenu();
    this.closeTableActionMenu();
    const context = this.linkContextForAnchor(anchor);
    if (!context || !this.linkContextMenu) return false;
    this.linkContext = context;
    this.linkContextReturnFocus = (
      document.activeElement instanceof HTMLElement
      && (document.activeElement === anchor || anchor.contains(document.activeElement))
    ) ? document.activeElement : null;
    this.preservedLinkSelection = {kind: 'text', anchor: context.from, head: context.to};
    this.refreshLinkContextMenuInteractionState();
    this.linkContextMenu.hidden = false;
    this.positionLinkContextMenu(point);
    window.requestAnimationFrame(() => focusCommandMenuItem(this.linkContextMenu, 0));
    return true;
  }

  closeLinkContextMenu(restoreFocus = false) {
    const context = this.linkContext;
    const returnFocus = this.linkContextReturnFocus;
    if (this.linkContextMenu) this.linkContextMenu.hidden = true;
    this.linkContext = null;
    this.linkContextReturnFocus = null;
    if (restoreFocus && context) {
      try {
        this.view.dispatch(this.view.state.tr.setSelection(TextSelection.create(this.view.state.doc, context.from, context.to)));
      } catch {}
      window.requestAnimationFrame(() => {
        if (returnFocus instanceof HTMLElement && returnFocus.isConnected) {
          returnFocus.focus({preventScroll: true});
        } else {
          focusWithoutScroll(this.view);
        }
      });
      this.refreshSurface();
    }
  }

  positionLinkContextMenu(point = null) {
    if (!this.linkContextMenu || this.linkContextMenu.hidden || !this.linkContext) return;
    const rect = this.linkContext.anchor.getBoundingClientRect();
    const menuRect = this.linkContextMenu.getBoundingClientRect();
    let preferredLeft = point?.x ?? rect.left;
    let preferredTop = point?.y ?? rect.bottom + SURFACE_GAP;
    if (point && preferredLeft + menuRect.width > window.innerWidth - VIEWPORT_MARGIN) preferredLeft -= menuRect.width;
    if (point && preferredTop + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) preferredTop -= menuRect.height;
    if (!point && preferredTop + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) preferredTop = rect.top - menuRect.height - SURFACE_GAP;
    const maxLeft = Math.max(VIEWPORT_MARGIN, window.innerWidth - menuRect.width - VIEWPORT_MARGIN);
    const maxTop = Math.max(VIEWPORT_MARGIN, window.innerHeight - menuRect.height - VIEWPORT_MARGIN);
    this.linkContextMenu.style.left = `${Math.round(clamp(preferredLeft, VIEWPORT_MARGIN, maxLeft))}px`;
    this.linkContextMenu.style.top = `${Math.round(clamp(preferredTop, VIEWPORT_MARGIN, maxTop))}px`;
  }

  openCurrentLink() {
    const href = this.linkContext?.href;
    if (!href) return false;
    this.closeLinkContextMenu(true);
    window.open(href, '_blank', 'noopener,noreferrer');
    return true;
  }

  selectCurrentLink() {
    const context = this.linkContext;
    if (!context) return false;
    const selection = TextSelection.create(this.view.state.doc, context.from, context.to);
    this.view.dispatch(this.view.state.tr.setSelection(selection));
    this.preservedLinkSelection = captureSelection(selection);
    return true;
  }

  editCurrentLink() {
    if (!this.editable || !this.selectCurrentLink()) return false;
    this.closeLinkContextMenu();
    document.getElementById('editor-link-trigger')?.click();
    return true;
  }

  copyCurrentLink() {
    const href = this.linkContext?.href;
    if (!href) return false;
    void copyPlainText(href);
    this.closeLinkContextMenu(true);
    return true;
  }

  removeCurrentLink() {
    if (!this.editable) return false;
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.removeCurrentLink());
    }
    const context = this.linkContext;
    if (!context) return false;
    let transaction = this.view.state.tr
      .removeMark(context.from, context.to, schema.marks.link);
    transaction = transaction.setSelection(
      TextSelection.create(transaction.doc, context.from, context.to),
    );
    this.view.dispatch(transaction.scrollIntoView());
    this.closeLinkContextMenu();
    focusWithoutScroll(this.view);
    this.refreshSurface();
    return true;
  }

  attachmentViewForElement(element) {
    if (!(element instanceof Element)) return null;
    for (const attachmentView of this.attachmentViews) {
      if (attachmentView?.dom === element) return attachmentView;
    }
    return null;
  }

  createAttachmentContextMenu() {
    const menu = document.createElement('div');
    menu.id = ATTACHMENT_CONTEXT_MENU_ID;
    menu.className = 'prosemirror-attachment-context-menu';
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Attachment actions');
    menu.hidden = true;
    menu.addEventListener('pointerdown', event => {
      event.stopPropagation();
    });
    menu.addEventListener('keydown', event => {
      handleCommandMenuKeyDown(event, menu, () => this.closeAttachmentContextMenu(true));
    });
    const actions = [
      {label: 'Open externally', action: 'open', requiresEditable: false, requiresAvailable: true},
      {label: 'Save as…', action: 'save', requiresEditable: false, requiresAvailable: true},
      {label: 'Reveal in Finder', action: 'reveal', requiresEditable: false, requiresAvailable: true},
      {label: 'Copy Image', action: 'copy_image', requiresEditable: false, requiresAvailable: true, imageOnly: true, separated: true},
      {label: 'Replace Image…', action: 'replace_image', requiresEditable: true, requiresAvailable: false, imageOnly: true},
      {label: 'Remove from note', action: 'remove', requiresEditable: true, requiresAvailable: false, danger: true, separated: true},
    ];
    for (const {
      label, action, requiresEditable, requiresAvailable, imageOnly = false, danger = false, separated = false,
    } of actions) {
      const button = document.createElement('button');
      button.type = 'button';
      button.setAttribute('role', 'menuitem');
      button.dataset.attachmentContextAction = action;
      button.dataset.requiresEditable = requiresEditable ? 'true' : 'false';
      button.dataset.requiresAvailable = requiresAvailable ? 'true' : 'false';
      button.dataset.imageOnly = imageOnly ? 'true' : 'false';
      if (danger) button.classList.add('danger');
      if (separated) button.classList.add('separated');
      button.textContent = label;
      button.addEventListener('pointerdown', event => {
        event.preventDefault();
        event.stopPropagation();
      });
      button.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        this.runAttachmentContextAction(action);
      });
      menu.appendChild(button);
    }
    document.body.appendChild(menu);
    this.attachmentContextMenu = menu;
  }

  refreshAttachmentContextMenuInteractionState() {
    if (!this.attachmentContextMenu || !this.attachmentContextView) return;
    const runtime = this.attachmentRegistry.get(this.attachmentContextView.node.attrs.attachmentId) || null;
    const available = runtime?.available === true;
    const isImage = this.attachmentContextView.node?.attrs?.kind === 'image';
    for (const button of this.attachmentContextMenu.querySelectorAll('button[role="menuitem"]')) {
      const requiresEditable = button.dataset.requiresEditable === 'true';
      const requiresAvailable = button.dataset.requiresAvailable === 'true';
      const imageOnly = button.dataset.imageOnly === 'true';
      button.hidden = imageOnly && !isImage;
      const disabled = (requiresEditable && !this.editable) || (requiresAvailable && !available);
      button.disabled = disabled;
      button.setAttribute('aria-disabled', disabled ? 'true' : 'false');
    }
  }

  openAttachmentContextMenu(attachmentView, point = null) {
    if (!attachmentView?.dom?.isConnected || !this.attachmentContextMenu) return false;
    this.closeLinkContextMenu();
    this.closeCodeContextMenu();
    this.closeTableActionMenu();
    this.attachmentContextView = attachmentView;
    this.attachmentContextReturnFocus = (
      document.activeElement instanceof HTMLElement
      && attachmentView.dom.contains(document.activeElement)
    ) ? document.activeElement : attachmentView.dom;
    const label = attachmentView.node?.attrs?.displayName || 'attachment';
    this.attachmentContextMenu.setAttribute('aria-label', `Actions for ${label}`);
    this.refreshAttachmentContextMenuInteractionState();
    this.attachmentContextMenu.hidden = false;
    this.positionAttachmentContextMenu(point);
    window.requestAnimationFrame(() => focusCommandMenuItem(this.attachmentContextMenu, 0));
    return true;
  }

  closeAttachmentContextMenu(restoreFocus = false) {
    const attachmentView = this.attachmentContextView;
    const returnFocus = this.attachmentContextReturnFocus;
    if (this.attachmentContextMenu) this.attachmentContextMenu.hidden = true;
    this.attachmentContextView = null;
    this.attachmentContextReturnFocus = null;
    if (!restoreFocus) return;
    window.requestAnimationFrame(() => {
      if (returnFocus instanceof HTMLElement && returnFocus.isConnected) {
        returnFocus.focus({preventScroll: true});
      } else if (attachmentView?.dom instanceof HTMLElement && attachmentView.dom.isConnected) {
        attachmentView.dom.focus({preventScroll: true});
      } else {
        focusWithoutScroll(this.view);
      }
    });
  }

  positionAttachmentContextMenu(point = null) {
    if (!this.attachmentContextMenu || this.attachmentContextMenu.hidden || !this.attachmentContextView?.dom?.isConnected) return;
    const anchorRect = this.attachmentContextView.dom.getBoundingClientRect();
    const menuRect = this.attachmentContextMenu.getBoundingClientRect();
    let left = point?.x ?? anchorRect.left;
    let top = point?.y ?? anchorRect.bottom + SURFACE_GAP;
    if (point && left + menuRect.width > window.innerWidth - VIEWPORT_MARGIN) left -= menuRect.width;
    if (point && top + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) top -= menuRect.height;
    if (!point && top + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) top = anchorRect.top - menuRect.height - SURFACE_GAP;
    const maxLeft = Math.max(VIEWPORT_MARGIN, window.innerWidth - menuRect.width - VIEWPORT_MARGIN);
    const maxTop = Math.max(VIEWPORT_MARGIN, window.innerHeight - menuRect.height - VIEWPORT_MARGIN);
    this.attachmentContextMenu.style.left = `${Math.round(clamp(left, VIEWPORT_MARGIN, maxLeft))}px`;
    this.attachmentContextMenu.style.top = `${Math.round(clamp(top, VIEWPORT_MARGIN, maxTop))}px`;
  }

  runAttachmentContextAction(action) {
    const attachmentView = this.attachmentContextView;
    if (!attachmentView) return false;
    if (action === 'remove') {
      if (!this.editable) return false;
      this.closeAttachmentContextMenu(false);
      const removed = attachmentView.deleteNode();
      if (removed) focusWithoutScroll(this.view);
      return removed;
    }
    let handled = false;
    if (action === 'open') handled = attachmentView.openOriginal();
    else if (action === 'save') handled = attachmentView.saveDisplayedAs();
    else if (action === 'reveal') handled = attachmentView.revealInFinder();
    else if (action === 'copy_image' && attachmentView.node?.attrs?.kind === 'image') {
      handled = true;
      void attachmentView.copyImage();
    } else if (action === 'replace_image' && attachmentView.node?.attrs?.kind === 'image') {
      if (!this.editable) return false;
      handled = attachmentView.replaceInput instanceof HTMLInputElement;
      if (handled) attachmentView.replaceInput.click();
    }
    if (handled) this.closeAttachmentContextMenu(true);
    return handled;
  }

  codeBlockContextForElement(element = null) {
    let codeElement = element instanceof Element ? element.closest('pre.prosemirror-code') : null;
    if (!(codeElement instanceof HTMLElement) || !this.root.contains(codeElement)) {
      const {$from} = this.view.state.selection;
      for (let depth = $from.depth; depth > 0; depth -= 1) {
        if ($from.node(depth).type === schema.nodes.code_block) {
          const position = $from.before(depth);
          const dom = this.view.nodeDOM(position);
          codeElement = dom instanceof HTMLElement && dom.matches('pre.prosemirror-code') ? dom : null;
          if (codeElement) return {element: codeElement, node: $from.node(depth), position};
        }
      }
      return null;
    }
    try {
      const base = this.view.posAtDOM(codeElement, 0);
      for (const position of [base, Math.min(base + 1, this.view.state.doc.content.size)]) {
        const $pos = this.view.state.doc.resolve(position);
        for (let depth = $pos.depth; depth > 0; depth -= 1) {
          if ($pos.node(depth).type === schema.nodes.code_block) {
            return {element: codeElement, node: $pos.node(depth), position: $pos.before(depth)};
          }
        }
      }
    } catch { /* detached/replaced DOM */ }
    return null;
  }

  createCodeContextMenu() {
    const menu = document.createElement('div');
    menu.id = CODE_CONTEXT_MENU_ID;
    menu.className = 'prosemirror-code-context-menu';
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Code block actions');
    menu.hidden = true;
    menu.addEventListener('pointerdown', event => event.stopPropagation());
    menu.addEventListener('mousedown', event => event.stopPropagation());
    menu.addEventListener('keydown', event => {
      handleCommandMenuKeyDown(event, menu, () => this.closeCodeContextMenu(true));
    });
    const actions = [
      ['Copy code', 'copy', false, false],
      ['Convert to normal text', 'normal', true, true],
      ['Delete code block', 'delete', true, true],
    ];
    for (const [label, action, requiresEditable, separated] of actions) {
      const button = document.createElement('button');
      button.type = 'button';
      button.setAttribute('role', 'menuitem');
      button.dataset.codeContextAction = action;
      button.dataset.requiresEditable = requiresEditable ? 'true' : 'false';
      if (separated && action === 'normal') button.classList.add('separated');
      if (action === 'delete') button.classList.add('danger');
      button.textContent = label;
      button.addEventListener('pointerdown', event => {
        event.preventDefault();
        event.stopPropagation();
      });
      button.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        this.runCodeContextAction(action);
      });
      menu.appendChild(button);
    }
    document.body.appendChild(menu);
    this.codeContextMenu = menu;
  }

  openCodeContextMenu(element, point = null) {
    if (!this.codeContextMenu) return false;
    const context = this.codeBlockContextForElement(element);
    if (!context) return false;
    this.closeLinkContextMenu();
    this.closeAttachmentContextMenu();
    this.closeTableActionMenu();
    this.codeContext = context;
    this.codeContextReturnFocus = document.activeElement instanceof HTMLElement
      ? document.activeElement
      : this.view.dom;
    for (const button of this.codeContextMenu.querySelectorAll('button[role="menuitem"]')) {
      const disabled = button.dataset.requiresEditable === 'true' && !this.editable;
      button.disabled = disabled;
      button.setAttribute('aria-disabled', disabled ? 'true' : 'false');
    }
    this.codeContextMenu.hidden = false;
    this.positionCodeContextMenu(point);
    window.requestAnimationFrame(() => focusCommandMenuItem(this.codeContextMenu, 0));
    return true;
  }

  closeCodeContextMenu(restoreFocus = false) {
    const returnFocus = this.codeContextReturnFocus;
    if (this.codeContextMenu) this.codeContextMenu.hidden = true;
    this.codeContext = null;
    this.codeContextReturnFocus = null;
    if (restoreFocus) {
      window.requestAnimationFrame(() => {
        if (returnFocus instanceof HTMLElement && returnFocus.isConnected) {
          returnFocus.focus({preventScroll: true});
        } else {
          focusWithoutScroll(this.view);
        }
      });
    }
  }

  positionCodeContextMenu(point = null) {
    if (!this.codeContextMenu || this.codeContextMenu.hidden || !this.codeContext?.element?.isConnected) return;
    const anchorRect = this.codeContext.element.getBoundingClientRect();
    const menuRect = this.codeContextMenu.getBoundingClientRect();
    let left = point?.x ?? anchorRect.left;
    let top = point?.y ?? anchorRect.bottom + SURFACE_GAP;
    if (point && left + menuRect.width > window.innerWidth - VIEWPORT_MARGIN) left -= menuRect.width;
    if (point && top + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) top -= menuRect.height;
    if (!point && top + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) top = anchorRect.top - menuRect.height - SURFACE_GAP;
    const maxLeft = Math.max(VIEWPORT_MARGIN, window.innerWidth - menuRect.width - VIEWPORT_MARGIN);
    const maxTop = Math.max(VIEWPORT_MARGIN, window.innerHeight - menuRect.height - VIEWPORT_MARGIN);
    this.codeContextMenu.style.left = `${Math.round(clamp(left, VIEWPORT_MARGIN, maxLeft))}px`;
    this.codeContextMenu.style.top = `${Math.round(clamp(top, VIEWPORT_MARGIN, maxTop))}px`;
  }

  runCodeContextAction(action) {
    const context = this.codeContext;
    if (!context) return false;
    if (action === 'copy') {
      const text = context.node.textContent;
      this.closeCodeContextMenu(true);
      void copyPlainText(text).then(copied => {
        this.showEditorFeedback(copied ? 'Code copied' : 'Could not copy code', !copied);
      });
      return true;
    }
    if (!this.editable) return false;
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.runCodeContextAction(action));
    }
    const current = this.codeBlockContextForElement(context.element);
    if (!current) {
      this.closeCodeContextMenu(false);
      return false;
    }
    const state = this.view.state;
    if (action === 'normal') {
      const cursor = Math.min(current.position + 1, state.doc.content.size);
      this.view.dispatch(state.tr.setSelection(TextSelection.near(state.doc.resolve(cursor), 1)));
      const command = commandForBlock('code', this.view.state);
      const applied = Boolean(command?.(
        this.view.state,
        transaction => this.view.dispatch(transaction),
        this.view,
      ));
      if (applied) {
        this.closeCodeContextMenu(false);
        focusWithoutScroll(this.view);
      }
      return applied;
    }
    if (action === 'delete') {
      const paragraph = schema.nodes.paragraph.create();
      let transaction = state.tr.replaceWith(
        current.position,
        current.position + current.node.nodeSize,
        paragraph,
      );
      const cursor = Math.min(current.position + 1, transaction.doc.content.size);
      transaction = transaction.setSelection(TextSelection.near(transaction.doc.resolve(cursor), 1)).scrollIntoView();
      this.view.dispatch(transaction);
      this.closeCodeContextMenu(false);
      focusWithoutScroll(this.view);
      return true;
    }
    return false;
  }

  linkUrlBeforeCursor() {
    const state = this.view.state;
    const {selection} = state;
    if (
      !selection.empty
      || scientificNodeContext(state)
      || selectionIsInsideNodeType(state, schema.nodes.code_block)
      || selectionUsesMark(state, schema.marks.code)
      || !selection.$from.parent.inlineContent
    ) {
      return false;
    }
    const text = selection.$from.parent.textBetween(0, selection.$from.parentOffset, null, '\ufffc');
    const result = autoLinkMatch(text, true);
    if (!result) return false;
    const from = selection.$from.start() + result.offset;
    const to = from + result.length;
    if (state.doc.rangeHasMark(from, to, schema.marks.link) || state.doc.rangeHasMark(from, to, schema.marks.note_link) || state.doc.rangeHasMark(from, to, schema.marks.media_link)) return false;
    this.view.dispatch(state.tr.addMark(
      from,
      to,
      schema.marks.link.create({href: result.href, title: null}),
    ));
    return true;
  }

  createFloatingToolbar() {
    const toolbar = document.createElement('div');
    toolbar.id = FLOATING_TOOLBAR_ID;
    toolbar.className = 'prosemirror-floating-toolbar';
    toolbar.setAttribute('role', 'toolbar');
    toolbar.setAttribute('aria-label', 'Selected text formatting');
    toolbar.hidden = true;
    const actions = [
      ['bold', 'B', 'Bold selected text'],
      ['italic', 'I', 'Italicise selected text'],
      ['underline', 'U', 'Underline selected text'],
      ['strikethrough', 'S', 'Strike through selected text'],
      ['highlight', 'Highlight', 'Highlight selected text'],
      ['link', 'Link', 'Add or edit link'],
    ];
    for (const [action, label, accessibleLabel] of actions) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = 'floating-format-button';
      button.dataset.liveEditorFormat = action;
      button.setAttribute('aria-label', accessibleLabel);
      button.setAttribute('aria-pressed', 'false');
      button.textContent = label;
      button.addEventListener('pointerdown', event => {
        event.preventDefault();
        event.stopPropagation();
      });
      button.addEventListener('mousedown', event => {
        event.preventDefault();
        event.stopPropagation();
      });
      button.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        if (action === 'link') {
          this.captureLinkSelection();
          this.hideFloatingToolbar();
          document.getElementById('editor-link-trigger')?.click();
          return;
        }
        this.format(action);
      });
      toolbar.appendChild(button);
    }
    document.body.appendChild(toolbar);
    this.floatingToolbar = toolbar;
  }

  hideFloatingToolbar() {
    if (this.floatingToolbar) {
      this.floatingToolbar.hidden = true;
    }
  }

  updateFloatingToolbar() {
    if (!this.floatingToolbar || this.destroyed || !this.editable) {
      return;
    }
    const {selection} = this.view.state;
    const activeElement = document.activeElement;
    const relevantFocus = this.view.hasFocus() || this.floatingToolbar.contains(activeElement);
    const inCodeBlock = ancestorDepth(this.view.state, ['code_block']) >= 0;
    if (
      !(selection instanceof TextSelection)
      || selection.empty
      || inCodeBlock
      || !relevantFocus
      || !this.root.isConnected
    ) {
      this.hideFloatingToolbar();
      return;
    }
    this.floatingToolbar.hidden = false;
    this.positionFloatingToolbar();
  }

  positionFloatingToolbar() {
    if (!this.floatingToolbar || this.floatingToolbar.hidden || this.destroyed) {
      return;
    }
    const {from, to} = this.view.state.selection;
    try {
      const start = this.view.coordsAtPos(from);
      const end = this.view.coordsAtPos(to);
      const toolbarRect = this.floatingToolbar.getBoundingClientRect();
      const selectionLeft = Math.min(start.left, end.left);
      const selectionRight = Math.max(start.right, end.right);
      const selectionTop = Math.min(start.top, end.top);
      const selectionBottom = Math.max(start.bottom, end.bottom);
      const centre = selectionLeft + (selectionRight - selectionLeft) / 2;
      const maxLeft = Math.max(VIEWPORT_MARGIN, window.innerWidth - toolbarRect.width - VIEWPORT_MARGIN);
      const left = clamp(centre - toolbarRect.width / 2, VIEWPORT_MARGIN, maxLeft);
      let top = selectionTop - toolbarRect.height - SURFACE_GAP;
      if (top < VIEWPORT_MARGIN) {
        top = selectionBottom + SURFACE_GAP;
      }
      const maxTop = Math.max(VIEWPORT_MARGIN, window.innerHeight - toolbarRect.height - VIEWPORT_MARGIN);
      this.floatingToolbar.style.left = `${Math.round(left)}px`;
      this.floatingToolbar.style.top = `${Math.round(clamp(top, VIEWPORT_MARGIN, maxTop))}px`;
    } catch {
      this.hideFloatingToolbar();
    }
  }

  createSurfaceButton(label, title, action, className = 'table-context-button') {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = className;
    button.textContent = label;
    button.title = title;
    button.setAttribute('aria-label', title);
    button.addEventListener('pointerdown', event => {
      event.preventDefault();
      event.stopPropagation();
    });
    button.addEventListener('mousedown', event => {
      event.preventDefault();
      event.stopPropagation();
    });
    button.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      action(button);
    });
    return button;
  }

  activateTableCellElement(cell) {
    if (!(cell instanceof HTMLElement) || !cell.matches('td, th') || !this.root.contains(cell)) return false;
    try {
      const base = this.view.posAtDOM(cell, 0);
      for (const position of [base, Math.min(base + 1, this.view.state.doc.content.size)]) {
        const $pos = this.view.state.doc.resolve(position);
        const $cell = cellAround($pos);
        if (!$cell) continue;
        const cursor = Math.min($cell.pos + 1, this.view.state.doc.content.size);
        this.view.dispatch(
          this.view.state.tr
            .setSelection(TextSelection.near(this.view.state.doc.resolve(cursor), 1))
            .setMeta('addToHistory', false),
        );
        return true;
      }
    } catch { /* detached/replaced table DOM */ }
    return false;
  }

  createTableSurfaces() {
    const toolbar = document.createElement('div');
    toolbar.id = TABLE_TOOLBAR_ID;
    toolbar.className = 'prosemirror-table-toolbar';
    toolbar.setAttribute('role', 'toolbar');
    toolbar.setAttribute('aria-label', 'Table editing');
    toolbar.hidden = true;
    const rowButton = this.createSurfaceButton(
      'Row +',
      'Insert row below',
      () => this.runTableCommand(addRowAfter),
    );
    const columnButton = this.createSurfaceButton(
      'Column +',
      'Insert column to the right',
      () => this.runTableCommand(addColumnAfter),
    );
    const headerButton = this.createSurfaceButton(
      'Header',
      'Toggle header row',
      () => this.runTableCommand(toggleActiveTableHeaderRow),
    );
    const moreButton = this.createSurfaceButton(
      'More',
      'More table actions',
      button => this.openTableActionMenu('table', button),
    );
    moreButton.setAttribute('aria-haspopup', 'menu');
    moreButton.setAttribute('aria-controls', TABLE_ACTION_MENU_ID);
    moreButton.setAttribute('aria-expanded', 'false');
    toolbar.append(rowButton, columnButton, headerButton, moreButton);
    document.body.appendChild(toolbar);
    this.tableToolbar = toolbar;

    this.tableLevelControl = this.createSurfaceButton('', 'Table options', button => {
      this.openTableActionMenu('table', button);
    }, 'table-edge-control table-level-control');
    this.tableLevelControl.id = TABLE_LEVEL_CONTROL_ID;
    this.tableLevelControl.setAttribute('aria-haspopup', 'menu');
    this.tableLevelControl.setAttribute('aria-controls', TABLE_ACTION_MENU_ID);
    this.tableLevelControl.setAttribute('aria-expanded', 'false');
    this.tableLevelControl.hidden = true;
    document.body.appendChild(this.tableLevelControl);

    this.tableRowControl = this.createSurfaceButton('', 'Row options', button => {
      this.openTableActionMenu('row', button);
    }, 'table-edge-control table-row-control');
    this.tableRowControl.id = TABLE_ROW_CONTROL_ID;
    this.tableRowControl.setAttribute('aria-haspopup', 'menu');
    this.tableRowControl.setAttribute('aria-controls', TABLE_ACTION_MENU_ID);
    this.tableRowControl.setAttribute('aria-expanded', 'false');
    this.tableRowControl.hidden = true;
    document.body.appendChild(this.tableRowControl);

    this.tableColumnControl = this.createSurfaceButton('', 'Column options', button => {
      this.openTableActionMenu('column', button);
    }, 'table-edge-control table-column-control');
    this.tableColumnControl.id = TABLE_COLUMN_CONTROL_ID;
    this.tableColumnControl.setAttribute('aria-haspopup', 'menu');
    this.tableColumnControl.setAttribute('aria-controls', TABLE_ACTION_MENU_ID);
    this.tableColumnControl.setAttribute('aria-expanded', 'false');
    this.tableColumnControl.hidden = true;
    document.body.appendChild(this.tableColumnControl);

    const menu = document.createElement('div');
    menu.id = TABLE_ACTION_MENU_ID;
    menu.className = 'prosemirror-table-action-menu';
    menu.setAttribute('role', 'menu');
    menu.setAttribute('aria-label', 'Table actions');
    menu.hidden = true;
    menu.addEventListener('pointerdown', event => event.stopPropagation());
    menu.addEventListener('mousedown', event => event.stopPropagation());
    menu.addEventListener('keydown', event => {
      handleCommandMenuKeyDown(event, menu, () => this.closeTableActionMenu(true));
    });
    document.body.appendChild(menu);
    this.tableActionMenu = menu;
  }

  hideTableSurfaces() {
    for (const surface of [
      this.tableToolbar,
      this.tableLevelControl,
      this.tableRowControl,
      this.tableColumnControl,
    ]) {
      if (surface) {
        surface.hidden = true;
      }
    }
    this.closeTableActionMenu();
  }

  updateTableSurfaces() {
    if (this.destroyed || !this.editable || !this.tableToolbar) {
      return;
    }
    const context = currentTableContext(this.view);
    const activeElement = document.activeElement;
    const surfaceFocus = [
      this.tableToolbar,
      this.tableLevelControl,
      this.tableRowControl,
      this.tableColumnControl,
      this.tableActionMenu,
    ].some(surface => surface instanceof HTMLElement && activeElement instanceof Node && surface.contains(activeElement));
    if (!context || (!this.view.hasFocus() && !surfaceFocus)) {
      this.hideTableSurfaces();
      return;
    }
    this.tableToolbar.hidden = false;
    this.tableLevelControl.hidden = false;
    this.tableRowControl.hidden = false;
    this.tableColumnControl.hidden = false;
    const buttons = this.tableToolbar.querySelectorAll('button');
    if (buttons[0]) buttons[0].disabled = !addRowAfter(this.view.state);
    if (buttons[1]) buttons[1].disabled = !addColumnAfter(this.view.state);
    if (buttons[2]) {
      buttons[2].disabled = !toggleActiveTableHeaderRow(this.view.state, undefined, this.view);
      buttons[2].classList.toggle('active', tableHasHeaderRow(context));
      buttons[2].setAttribute('aria-pressed', tableHasHeaderRow(context) ? 'true' : 'false');
    }
    this.scheduleTableSurfacePosition();
  }

  scheduleTableSurfacePosition() {
    if (this.destroyed || this.tableGeometryFrame !== null) {
      return;
    }
    this.tableGeometryFrame = window.requestAnimationFrame(() => {
      this.tableGeometryFrame = null;
      this.positionTableSurfaces();
    });
  }

  positionTableSurfaces() {
    if (!this.tableToolbar || this.tableToolbar.hidden || this.destroyed) {
      return;
    }
    const dom = tableDomContext(this.view);
    if (!dom) {
      this.hideTableSurfaces();
      return;
    }
    const tableRect = dom.tableElement.getBoundingClientRect();
    const rowRect = dom.row.getBoundingClientRect();
    const cellRect = dom.cell.getBoundingClientRect();
    const toolbarRect = this.tableToolbar.getBoundingClientRect();
    let toolbarTop = tableRect.top - toolbarRect.height - SURFACE_GAP;
    if (toolbarTop < VIEWPORT_MARGIN) {
      toolbarTop = tableRect.bottom + SURFACE_GAP;
    }
    const toolbarLeft = clamp(
      tableRect.right - toolbarRect.width,
      VIEWPORT_MARGIN,
      Math.max(VIEWPORT_MARGIN, window.innerWidth - toolbarRect.width - VIEWPORT_MARGIN),
    );
    this.tableToolbar.style.left = `${Math.round(toolbarLeft)}px`;
    this.tableToolbar.style.top = `${Math.round(clamp(toolbarTop, VIEWPORT_MARGIN, window.innerHeight - toolbarRect.height - VIEWPORT_MARGIN))}px`;

    const placeControl = (control, left, top) => {
      if (!control) return;
      const rect = control.getBoundingClientRect();
      control.style.left = `${Math.round(clamp(left, VIEWPORT_MARGIN, window.innerWidth - rect.width - VIEWPORT_MARGIN))}px`;
      control.style.top = `${Math.round(clamp(top, VIEWPORT_MARGIN, window.innerHeight - rect.height - VIEWPORT_MARGIN))}px`;
    };
    placeControl(this.tableLevelControl, tableRect.left - 6, tableRect.top - 6);
    placeControl(this.tableRowControl, tableRect.left - 3, rowRect.top + rowRect.height / 2 - 11);
    placeControl(this.tableColumnControl, cellRect.left + cellRect.width / 2 - 11, tableRect.top - 3);
    if (this.tableActionMenuKind !== null) {
      this.positionTableActionMenu();
    }
  }

  tableMenuItems(kind) {
    if (kind === 'row') {
      return [
        ['Insert row above', addRowBefore, false],
        ['Insert row below', addRowAfter, false],
        ['Delete row', deleteRow, true],
      ];
    }
    if (kind === 'column') {
      return [
        ['Insert column left', addColumnBefore, false],
        ['Insert column right', addColumnAfter, false],
        ['Delete column', deleteColumn, true],
      ];
    }
    return [
      [tableHasHeaderRow(currentTableContext(this.view)) ? 'Remove header row' : 'Use header row', toggleActiveTableHeaderRow, false],
      ['Distribute columns evenly', (state, dispatch, view) => setActiveTableColumnWidths(state, dispatch, view, 'equal'), false],
      ['Fit table to canvas', (state, dispatch, view) => setActiveTableColumnWidths(state, dispatch, view, 'fit'), false],
      ['Delete table', deleteActiveTable, true],
    ];
  }

  openTableActionMenu(kind, anchor, point = null) {
    if (!this.tableActionMenu || !currentTableContext(this.view)) {
      return false;
    }
    this.closeLinkContextMenu();
    this.closeAttachmentContextMenu();
    this.closeCodeContextMenu();
    this.tableActionMenuKind = kind;
    this.tableActionMenuSelection = captureSelection(this.view.state.selection);
    this.tableActionMenu.dataset.kind = kind;
    this.tableActionMenu.replaceChildren();
    for (const [label, command, danger] of this.tableMenuItems(kind)) {
      const button = document.createElement('button');
      button.type = 'button';
      button.className = danger ? 'table-action-item danger' : 'table-action-item';
      button.setAttribute('role', 'menuitem');
      button.textContent = label;
      button.disabled = !this.editable || !command(this.view.state, undefined, this.view);
      button.addEventListener('pointerdown', event => {
        event.preventDefault();
        event.stopPropagation();
      });
      button.addEventListener('mousedown', event => {
        event.preventDefault();
        event.stopPropagation();
      });
      button.addEventListener('click', event => {
        event.preventDefault();
        event.stopPropagation();
        this.runTableCommand(command);
      });
      this.tableActionMenu.appendChild(button);
    }
    if (this.tableActionMenuAnchor?.matches?.('button')) {
      this.tableActionMenuAnchor.setAttribute('aria-expanded', 'false');
    }
    this.tableActionMenuAnchor = anchor;
    if (this.tableActionMenuAnchor?.matches?.('button')) {
      this.tableActionMenuAnchor.setAttribute('aria-expanded', 'true');
    }
    this.tableActionMenu.dataset.anchorId = anchor.id || '';
    this.tableActionMenu.hidden = false;
    this.positionTableActionMenu(anchor, point);
    window.requestAnimationFrame(() => focusCommandMenuItem(this.tableActionMenu, 0));
    return true;
  }

  positionTableActionMenu(explicitAnchor = null, point = null) {
    if (!this.tableActionMenu || this.tableActionMenu.hidden) {
      return;
    }
    const stored = this.tableActionMenu.dataset.anchorId;
    const anchor = explicitAnchor || this.tableActionMenuAnchor || (stored ? document.getElementById(stored) : null);
    if (!(anchor instanceof HTMLElement)) {
      this.closeTableActionMenu();
      return;
    }
    const anchorRect = anchor.getBoundingClientRect();
    const menuRect = this.tableActionMenu.getBoundingClientRect();
    let left = point?.x ?? anchorRect.left;
    let top = point?.y ?? anchorRect.bottom + 6;
    if (left + menuRect.width > window.innerWidth - VIEWPORT_MARGIN) {
      left = point ? left - menuRect.width : anchorRect.right - menuRect.width;
    }
    if (top + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) {
      top = point ? top - menuRect.height : anchorRect.top - menuRect.height - 6;
    }
    this.tableActionMenu.style.left = `${Math.round(clamp(left, VIEWPORT_MARGIN, window.innerWidth - menuRect.width - VIEWPORT_MARGIN))}px`;
    this.tableActionMenu.style.top = `${Math.round(clamp(top, VIEWPORT_MARGIN, window.innerHeight - menuRect.height - VIEWPORT_MARGIN))}px`;
  }

  closeTableActionMenu(restoreFocus = false) {
    const anchor = this.tableActionMenuAnchor;
    const selection = this.tableActionMenuSelection;
    this.tableActionMenuKind = null;
    if (anchor?.matches?.('button')) anchor.setAttribute('aria-expanded', 'false');
    this.tableActionMenuAnchor = null;
    this.tableActionMenuSelection = null;
    if (this.tableActionMenu) {
      this.tableActionMenu.hidden = true;
      this.tableActionMenu.replaceChildren();
      delete this.tableActionMenu.dataset.anchorId;
    }
    if (restoreFocus) {
      restoreSelection(this.view, selection);
      if (
        anchor instanceof HTMLElement
        && anchor.isConnected
        && anchor.matches('button, [href], input, select, textarea, [tabindex]:not([tabindex="-1"])')
      ) anchor.focus({preventScroll: true});
      else focusWithoutScroll(this.view);
      this.refreshSurface();
    }
  }

  runTableCommand(command) {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.runTableCommand(command));
    }
    if (!this.editable || !command || !currentTableContext(this.view)) {
      return false;
    }
    const applied = Boolean(command(
      this.view.state,
      transaction => this.view.dispatch(transaction),
      this.view,
    ));
    if (applied) {
      this.closeTableActionMenu();
      focusWithoutScroll(this.view);
      this.refreshSurface();
    }
    return applied;
  }

  createInsertMenu() {
    const menu = document.createElement('div');
    menu.id = INSERT_MENU_ID;
    menu.className = 'prosemirror-insert-menu';
    menu.hidden = true;
    menu.addEventListener('pointerdown', event => event.stopPropagation());
    menu.addEventListener('mousedown', event => event.stopPropagation());
    document.body.appendChild(menu);
    this.insertMenu = menu;
  }

  filteredInsertCommands(query) {
    const insideTable = selectionIsInsideTable(this.view.state);
    const commands = this.insertMenuSource === 'slash' ? SLASH_COMMANDS : INSERT_COMMANDS;
    const linkTrigger = document.getElementById('editor-link-trigger');
    const linkAvailable = linkTrigger instanceof HTMLButtonElement && !linkTrigger.disabled;
    return commands.filter(command => (
      (!insideTable || command.action !== 'table')
      && (command.action !== 'link' || linkAvailable)
      && commandMatchesQuery(command, query)
    ));
  }

  renderInsertMenu() {
    if (!this.insertMenu || this.insertMenuSource === null) {
      return;
    }
    if (this.tableGridOpen) {
      this.insertMenu.dataset.surface = 'table-grid';
      this.renderTableGrid();
      return;
    }
    if (this.scientificToolsOpen) {
      this.insertMenu.dataset.surface = 'scientific-tools';
      this.renderScientificTools();
      return;
    }
    if (this.scientificComposerKind !== null) {
      this.renderScientificComposer();
      return;
    }
    this.insertMenu.dataset.surface = 'command-list';
    const slashOwned = this.insertMenuSource === 'slash';
    configureFloatingSurface(this.insertMenu, {
      role: slashOwned ? 'listbox' : 'menu',
      label: slashOwned ? 'Editor commands' : 'Insert',
    });
    document.getElementById(INSERT_TRIGGER_ID)?.setAttribute('aria-haspopup', 'menu');
    const commands = this.filteredInsertCommands(this.insertMenuQuery);
    this.insertMenuCommands = commands;
    this.insertMenuIndex = commands.length === 0
      ? 0
      : clamp(this.insertMenuIndex, 0, commands.length - 1);
    this.insertMenu.replaceChildren();

    const header = document.createElement('div');
    header.className = 'insert-menu-header';
    const title = document.createElement('strong');
    title.textContent = this.insertMenuSource === 'slash'
      ? `Command /${this.insertMenuQuery}`
      : 'Insert';
    const hint = document.createElement('span');
    hint.textContent = 'Up/Down navigate | Enter insert | Esc close';
    header.append(title, hint);
    this.insertMenu.appendChild(header);

    const list = document.createElement('div');
    list.className = 'insert-menu-list';
    if (commands.length === 0) {
      const empty = document.createElement('p');
      empty.className = 'insert-menu-empty';
      empty.textContent = 'No matching blocks';
      list.appendChild(empty);
    } else {
      commands.forEach((command, index) => {
        const button = document.createElement('button');
        button.type = 'button';
        button.id = `${INSERT_MENU_ID}-item-${index}`;
        button.className = 'insert-menu-item';
        button.setAttribute('role', slashOwned ? 'option' : 'menuitem');
        if (slashOwned) button.setAttribute('aria-selected', index === this.insertMenuIndex ? 'true' : 'false');
        button.dataset.active = index === this.insertMenuIndex ? 'true' : 'false';
        button.dataset.insertCommand = command.id;
        button.tabIndex = slashOwned ? -1 : (index === this.insertMenuIndex ? 0 : -1);

        const icon = document.createElement('span');
        icon.className = 'insert-menu-icon';
        icon.setAttribute('aria-hidden', 'true');
        icon.textContent = command.icon;
        const copy = document.createElement('span');
        copy.className = 'insert-menu-copy';
        const label = document.createElement('strong');
        label.textContent = command.label;
        const description = document.createElement('small');
        description.textContent = command.description;
        copy.append(label, description);
        button.append(icon, copy);
        button.addEventListener('pointerdown', event => {
          event.preventDefault();
          event.stopPropagation();
        });
        button.addEventListener('mousedown', event => {
          event.preventDefault();
          event.stopPropagation();
        });
        button.addEventListener('mouseenter', () => {
          this.insertMenuIndex = index;
          this.updateInsertMenuActiveItem();
        });
        button.addEventListener('click', event => {
          event.preventDefault();
          event.stopPropagation();
          this.executeInsertCommand(command.id);
        });
        list.appendChild(button);
      });
    }
    this.insertMenu.appendChild(list);

    this.insertMenu.hidden = false;
    if (slashOwned) {
      this.view.dom.setAttribute('aria-controls', INSERT_MENU_ID);
      this.view.dom.setAttribute('aria-expanded', 'true');
      this.view.dom.setAttribute('aria-autocomplete', 'list');
    } else {
      this.view.dom.removeAttribute('aria-controls');
      this.view.dom.removeAttribute('aria-expanded');
      this.view.dom.removeAttribute('aria-activedescendant');
      this.view.dom.removeAttribute('aria-autocomplete');
    }
    this.updateInsertMenuActiveItem();
    this.positionInsertMenu();
    if (!slashOwned) window.requestAnimationFrame(() => focusCommandMenuItem(this.insertMenu, this.insertMenuIndex));
  }

  openTableGrid() {
    if (selectionIsInsideTable(this.view.state)) {
      return false;
    }
    this.tableGridOpen = true;
    this.scientificComposerKind = null;
    this.tableGridRows = 3;
    this.tableGridColumns = 3;
    this.tableGridHeader = true;
    this.renderInsertMenu();
    return true;
  }

  renderTableGrid() {
    if (!this.insertMenu) {
      return;
    }
    this.insertMenu.replaceChildren();
    const header = document.createElement('div');
    header.className = 'insert-menu-header';
    const title = document.createElement('strong');
    title.id = `${INSERT_MENU_ID}-table-title`;
    title.textContent = 'Insert table';
    const hint = document.createElement('span');
    hint.id = `${INSERT_MENU_ID}-table-description`;
    hint.textContent = 'Arrow keys size | Space header | Enter insert | Esc back';
    document.getElementById(INSERT_TRIGGER_ID)?.setAttribute('aria-haspopup', 'dialog');
    configureFloatingSurface(this.insertMenu, {
      role: 'dialog',
      labelledBy: title.id,
      describedBy: hint.id,
      modal: false,
    });
    header.append(title, hint);
    this.insertMenu.appendChild(header);

    const body = document.createElement('div');
    body.className = 'table-grid-scroll-body';
    this.insertMenu.appendChild(body);

    const status = document.createElement('div');
    status.className = 'table-grid-status';
    status.setAttribute('aria-live', 'polite');
    status.textContent = `${this.tableGridRows} x ${this.tableGridColumns} table`;
    body.appendChild(status);

    const grid = document.createElement('div');
    grid.className = 'table-grid-picker';
    grid.setAttribute('role', 'grid');
    grid.setAttribute('aria-label', 'Choose table dimensions');
    for (let row = 1; row <= TABLE_GRID_SIZE; row += 1) {
      for (let column = 1; column <= TABLE_GRID_SIZE; column += 1) {
        const cell = document.createElement('button');
        cell.type = 'button';
        cell.className = 'table-grid-cell';
        cell.setAttribute('role', 'gridcell');
        cell.setAttribute('aria-label', `${row} rows by ${column} columns`);
        cell.dataset.row = String(row);
        cell.dataset.column = String(column);
        cell.dataset.active = row <= this.tableGridRows && column <= this.tableGridColumns ? 'true' : 'false';
        cell.tabIndex = row === this.tableGridRows && column === this.tableGridColumns ? 0 : -1;
        cell.addEventListener('pointerdown', event => {
          event.preventDefault();
          event.stopPropagation();
        });
        cell.addEventListener('mouseenter', () => {
          this.tableGridRows = row;
          this.tableGridColumns = column;
          this.updateTableGridState();
        });
        cell.addEventListener('click', event => {
          event.preventDefault();
          event.stopPropagation();
          this.tableGridRows = row;
          this.tableGridColumns = column;
          this.insertTableFromGrid();
        });
        grid.appendChild(cell);
      }
    }
    body.appendChild(grid);

    const options = document.createElement('div');
    options.className = 'table-grid-options';
    const headerToggle = document.createElement('button');
    headerToggle.type = 'button';
    headerToggle.className = 'table-grid-header-toggle active';
    headerToggle.setAttribute('aria-pressed', 'true');
    headerToggle.textContent = 'Header row';
    headerToggle.addEventListener('pointerdown', event => event.preventDefault());
    headerToggle.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      this.tableGridHeader = !this.tableGridHeader;
      this.updateTableGridState();
    });
    const cancel = document.createElement('button');
    cancel.type = 'button';
    cancel.className = 'table-grid-back';
    cancel.textContent = 'Back';
    cancel.addEventListener('pointerdown', event => event.preventDefault());
    cancel.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      this.returnFromTableGrid();
    });
    options.append(headerToggle, cancel);
    body.appendChild(options);
    this.insertMenu.hidden = false;
    this.positionInsertMenu();
    window.requestAnimationFrame(() => {
      this.insertMenu?.querySelector(`.table-grid-cell[data-row="${this.tableGridRows}"][data-column="${this.tableGridColumns}"]`)?.focus({preventScroll: true});
    });
  }

  returnFromTableGrid() {
    this.tableGridOpen = false;
    const commands = this.filteredInsertCommands(this.insertMenuQuery);
    this.insertMenuIndex = Math.max(0, commands.findIndex(command => command.action === 'table'));
    this.renderInsertMenu();
    if (this.insertMenuSource === 'slash') {
      window.requestAnimationFrame(() => focusWithoutScroll(this.view));
    }
    return true;
  }

  updateTableGridState() {
    if (!this.insertMenu || !this.tableGridOpen) {
      return;
    }
    const gridHadFocus = document.activeElement instanceof Element && Boolean(document.activeElement.closest('.table-grid-picker'));
    let selectedCell = null;
    this.insertMenu.querySelectorAll('.table-grid-cell').forEach(cell => {
      const row = Number(cell.dataset.row);
      const column = Number(cell.dataset.column);
      const selected = row === this.tableGridRows && column === this.tableGridColumns;
      cell.dataset.active = row <= this.tableGridRows && column <= this.tableGridColumns ? 'true' : 'false';
      cell.tabIndex = selected ? 0 : -1;
      if (selected) selectedCell = cell;
    });
    if (gridHadFocus && selectedCell instanceof HTMLElement) selectedCell.focus({preventScroll: true});
    const status = this.insertMenu.querySelector('.table-grid-status');
    if (status) {
      status.textContent = `${this.tableGridRows} x ${this.tableGridColumns} table`;
    }
    const toggle = this.insertMenu.querySelector('.table-grid-header-toggle');
    if (toggle) {
      toggle.classList.toggle('active', this.tableGridHeader);
      toggle.setAttribute('aria-pressed', this.tableGridHeader ? 'true' : 'false');
    }
  }

  insertTableFromGrid() {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.insertTableFromGrid());
    }
    const slash = this.insertMenuSource === 'slash' ? this.slashContext : null;
    const slashText = slash ? `/${slash.query}` : '';
    this.suppressSlashMenu = true;
    if (slash) {
      try {
        let transaction = this.view.state.tr.delete(slash.from, slash.to);
        transaction = transaction.setSelection(TextSelection.near(transaction.doc.resolve(slash.from)));
        this.view.dispatch(transaction);
      } catch {
        this.suppressSlashMenu = false;
        return false;
      }
    } else if (!restoreSelection(this.view, this.preservedSelection)) {
      this.suppressSlashMenu = false;
      return false;
    }
    const rows = this.tableGridRows;
    const columns = this.tableGridColumns;
    const headerRow = this.tableGridHeader;
    this.closeInsertMenu(false);
    const applied = insertTable(rows, columns, headerRow)(
      this.view.state,
      transaction => this.view.dispatch(transaction),
      this.view,
    );
    if (!applied && slash && slashText !== '') {
      const position = this.view.state.selection.from;
      this.view.dispatch(this.view.state.tr.insertText(slashText, position, position));
    }
    this.suppressSlashMenu = false;
    if (applied) {
      focusWithoutScroll(this.view);
      this.refreshSurface();
    }
    return applied;
  }

  /** @returns {HTMLElement[]} */
  scientificToolsItems() {
    return this.scientificToolsOpen ? enabledCommandMenuItems(this.insertMenu) : [];
  }

  refreshScientificToolsFocusables(preferredIndex = this.scientificToolsFocusIndex) {
    const items = this.scientificToolsItems();
    if (items.length === 0) {
      this.scientificToolsFocusIndex = 0;
      return items;
    }
    const activeElement = document.activeElement;
    const activeIndex = activeElement instanceof HTMLElement ? items.indexOf(activeElement) : -1;
    const index = clamp(activeIndex >= 0 ? activeIndex : preferredIndex, 0, items.length - 1);
    this.scientificToolsFocusIndex = index;
    items.forEach((item, itemIndex) => { item.tabIndex = itemIndex === index ? 0 : -1; });
    return items;
  }

  focusScientificToolsItem(index = 0) {
    const items = this.refreshScientificToolsFocusables(index);
    if (items.length === 0) return false;
    const target = clamp(index, 0, items.length - 1);
    this.scientificToolsFocusIndex = target;
    items.forEach((item, itemIndex) => { item.tabIndex = itemIndex === target ? 0 : -1; });
    items[target].focus({preventScroll: true});
    return true;
  }

  handleScientificToolsKeyDown(event) {
    if (!this.scientificToolsOpen || this.scientificComposerKind !== null) return false;
    return handleCommandMenuKeyDown(event, this.insertMenu, () => {
      this.closeInsertMenu(false);
      restoreSelection(this.view, this.preservedSelection);
      focusWithoutScroll(this.view);
      this.refreshSurface();
    });
  }

  openScientificTools(anchor) {
    if (this.destroyed || !(anchor instanceof Element)) return false;
    const context = scientificNodeContext(this.view.state);
    if (!context) return false;
    this.preservedSelection = captureSelection(this.view.state.selection);
    this.dismissedSlashPosition = null;
    this.slashContext = null;
    this.insertMenuSource = 'scientific';
    this.insertMenuQuery = '';
    this.insertMenuIndex = 0;
    this.tableGridOpen = false;
    this.scientificComposerKind = null;
    this.scientificToolsOpen = true;
    this.scientificToolsAnchor = anchor;
    this.scientificToolsFocusIndex = 0;
    this.renderInsertMenu();
    window.requestAnimationFrame(() => this.focusScientificToolsItem(0));
    return true;
  }

  renderScientificTools() {
    if (!this.insertMenu || !this.scientificToolsOpen) return;
    const activeContext = scientificNodeContext(this.view.state);
    if (!activeContext) {
      this.closeInsertMenu(false);
      return;
    }
    this.insertMenu.replaceChildren();
    const header = document.createElement('div');
    header.className = 'insert-menu-header';
    const title = document.createElement('strong');
    title.id = SCIENTIFIC_TOOLS_TITLE_ID;
    title.textContent = 'Scientific tools';
    const hint = document.createElement('span');
    hint.id = SCIENTIFIC_TOOLS_DESCRIPTION_ID;
    hint.textContent = 'Use arrow keys to move through actions. Escape closes the tools.';
    configureFloatingSurface(this.insertMenu, {
      role: 'menu',
      labelledBy: SCIENTIFIC_TOOLS_TITLE_ID,
      describedBy: SCIENTIFIC_TOOLS_DESCRIPTION_ID,
    });
    header.append(title, hint);
    this.insertMenu.appendChild(header);

    const body = document.createElement('div');
    body.className = 'scientific-context-tools';
    const insertSource = source => {
      this.closeInsertMenu(false);
      this.scientific('template', source);
    };
    const openGuided = kind => this.openGuidedStructure(kind, 'active');

    const editVisually = document.createElement('button');
    editVisually.type = 'button';
    editVisually.className = 'scientific-context-edit';
    editVisually.textContent = 'Edit visually';
    editVisually.setAttribute('role', 'menuitem');
    editVisually.addEventListener('pointerdown', event => event.preventDefault());
    editVisually.addEventListener('click', event => {
      event.preventDefault();
      const context = scientificNodeContext(this.view.state);
      if (!context) return;
      const source = context.node.textContent;
      const chemistry = guidedScientificEligibility(source, true).lossless;
      const compositeChemistry = /\\(?:ce|pu)\s*\{/.test(source) && !chemistry;
      this.openScientificComposer(chemistry ? 'chemistry' : 'equation', {
        initialValue: source,
        displayMode: context.node.type === schema.nodes.math_display,
        target: 'edit',
        viewMode: compositeChemistry ? 'source' : undefined,
      });
    });

    const editSource = document.createElement('button');
    editSource.type = 'button';
    editSource.className = 'scientific-context-edit';
    editSource.textContent = 'Edit advanced source';
    editSource.setAttribute('role', 'menuitem');
    editSource.setAttribute('aria-label', 'Edit raw scientific source');
    editSource.addEventListener('pointerdown', event => event.preventDefault());
    editSource.addEventListener('click', event => {
      event.preventDefault();
      const context = scientificNodeContext(this.view.state);
      if (!context) return;
      this.closeInsertMenu(false);
      this.view.dispatch(
        this.view.state.tr
          .setSelection(TextSelection.create(this.view.state.doc, context.position + 1))
          .scrollIntoView(),
      );
      focusWithoutScroll(this.view);
      this.refreshSurface();
    });

    const convert = document.createElement('button');
    convert.type = 'button';
    convert.className = 'scientific-context-edit scientific-context-convert';
    convert.textContent = activeContext.node.type === schema.nodes.math_inline
      ? 'Convert to display equation'
      : 'Convert to inline equation';
    convert.setAttribute('role', 'menuitem');
    convert.setAttribute('aria-label', convert.textContent);
    convert.addEventListener('pointerdown', event => event.preventDefault());
    convert.addEventListener('click', event => {
      event.preventDefault();
      this.convertActiveScientificNode();
    });

    const copyGroup = document.createElement('details');
    copyGroup.className = 'scientific-tool-group scientific-context-copy-group';
    const copySummary = document.createElement('summary');
    copySummary.textContent = 'Copy…';
    copySummary.setAttribute('role', 'menuitem');
    const copyActions = document.createElement('div');
    copyActions.className = 'scientific-context-copy-actions';
    const copySource = document.createElement('button');
    copySource.type = 'button';
    copySource.textContent = 'LaTeX source';
    copySource.setAttribute('role', 'menuitem');
    const copyDelimited = document.createElement('button');
    copyDelimited.type = 'button';
    copyDelimited.textContent = 'With delimiters';
    copyDelimited.setAttribute('role', 'menuitem');
    for (const button of [copySource, copyDelimited]) {
      button.addEventListener('pointerdown', event => event.preventDefault());
    }
    copySource.addEventListener('click', event => {
      event.preventDefault();
      this.copyScientific('source');
    });
    copyDelimited.addEventListener('click', event => {
      event.preventDefault();
      this.copyScientific('delimited');
    });
    copyActions.append(copySource, copyDelimited);
    copyGroup.append(copySummary, copyActions);

    const deleteEquation = document.createElement('button');
    deleteEquation.type = 'button';
    deleteEquation.className = 'scientific-context-delete';
    deleteEquation.textContent = 'Delete equation';
    deleteEquation.setAttribute('role', 'menuitem');
    deleteEquation.setAttribute('aria-label', 'Delete the entire equation');
    deleteEquation.addEventListener('pointerdown', event => event.preventDefault());
    deleteEquation.addEventListener('click', event => {
      event.preventDefault();
      const context = scientificNodeContext(this.view.state);
      if (!context) return;
      this.closeInsertMenu(false);
      const selection = NodeSelection.create(this.view.state.doc, context.position);
      this.runDiscreteUserAction(() => {
        this.view.dispatch(
          this.view.state.tr
            .setSelection(selection)
            .deleteSelection()
            .scrollIntoView(),
        );
        return true;
      });
      focusWithoutScroll(this.view);
      this.refreshSurface();
    });

    const notationTitle = document.createElement('strong');
    notationTitle.className = 'scientific-context-section-title';
    notationTitle.textContent = activeScientificInsertionTarget(this.view.state) === 'active_caret'
      ? 'Insert notation at cursor'
      : 'Add notation to equation';

    const close = document.createElement('button');
    close.type = 'button';
    close.className = 'table-grid-back scientific-context-close';
    close.textContent = 'Close';
    close.setAttribute('role', 'menuitem');
    close.addEventListener('pointerdown', event => event.preventDefault());
    close.addEventListener('click', event => {
      event.preventDefault();
      this.closeInsertMenu(false);
      focusWithoutScroll(this.view);
    });

    if (this.editable) {
      body.append(
        editVisually,
        editSource,
        convert,
        copyGroup,
        deleteEquation,
        notationTitle,
        scientificToolGroupsPanel(SCIENTIFIC_TEMPLATE_GROUPS, insertSource, openGuided, true),
        scientificToolGroupsPanel(SCIENTIFIC_SYMBOL_GROUPS, insertSource, openGuided, true),
        close,
      );
    } else {
      body.append(copyGroup, close);
    }
    this.insertMenu.appendChild(body);
    this.insertMenu.hidden = false;
    this.insertMenu.querySelectorAll('details').forEach(details => {
      details.addEventListener('toggle', () => {
        this.refreshScientificToolsFocusables();
        this.positionInsertMenu();
      });
    });
    this.refreshScientificToolsFocusables();
    this.positionInsertMenu();
  }

  openMatrixGrid(mode = 'display') {
    if (mode === 'composer') {
      return this.openGuidedStructure('matrix', 'composer');
    }
    if (mode === 'scientific') {
      return this.openGuidedStructure('matrix', 'active');
    }
    return this.openScientificComposer('equation', {
      guidedKind: 'matrix',
      displayMode: true,
      target: 'new',
    });
  }

  openGuidedStructure(requestedKind, origin = 'composer', fieldInsertion = null) {
    const [kind, variant] = resolveGuidedStructure(requestedKind);
    if (!Object.prototype.hasOwnProperty.call(GUIDED_STRUCTURE_LABELS, kind)) return false;
    if (origin === 'active') {
      const context = scientificNodeContext(this.view.state);
      const target = activeScientificInsertionTarget(this.view.state);
      if (!context || !target) return false;
      return this.openScientificComposer('equation', {
        guidedKind: kind,
        guidedVariant: variant,
        displayMode: context.node.type === schema.nodes.math_display,
        target,
      });
    }
    const currentSource = this.scientificComposerInitialValue;
    const emptyWholeEquation = currentSource.trim() === '' && this.scientificComposerTarget === 'new';
    if (emptyWholeEquation) {
      return this.openScientificComposer(this.scientificComposerKind || 'equation', {
        guidedKind: kind,
        guidedVariant: variant,
        displayMode: this.scientificComposerDisplay,
        target: 'new',
      });
    }
    const returnState = {
      kind: this.scientificComposerKind,
      displayMode: this.scientificComposerDisplay,
      source: currentSource,
      originalText: this.scientificComposerOriginalText,
      smartPaste: this.scientificComposerSmartPaste,
      selectionStart: this.scientificComposerSelectionStart,
      selectionEnd: this.scientificComposerSelectionEnd,
      viewMode: this.scientificComposerViewMode,
      guidedKind: this.scientificComposerGuidedKind,
      guidedData: cloneGuidedData(this.scientificComposerGuidedData),
      target: this.scientificComposerTarget,
      fieldInsertion,
    };
    return this.openScientificComposer('equation', {
      guidedKind: kind,
      guidedVariant: variant,
      displayMode: this.scientificComposerDisplay,
      target: 'composer_insert',
      returnState,
    });
  }

  openScientificComposer(kind, options = {}) {
    this.tableGridOpen = false;
    this.scientificToolsOpen = false;
    if (this.insertMenuSource !== 'scientific' || !this.scientificToolsAnchor?.isConnected) {
      this.scientificToolsAnchor = null;
    }
    this.scientificComposerKind = kind === 'chemistry' ? 'chemistry' : 'equation';
    this.scientificComposerDisplay = options.displayMode ?? (this.scientificComposerKind === 'chemistry');
    this.scientificComposerOriginalText = String(options.originalText ?? options.initialValue ?? '');
    this.scientificComposerSmartPaste = Boolean(options.smartPaste);
    this.scientificComposerSelectionStart = Number.isFinite(options.selectionStart) ? options.selectionStart : null;
    this.scientificComposerSelectionEnd = Number.isFinite(options.selectionEnd) ? options.selectionEnd : null;
    this.scientificComposerTarget = ['active_append', 'active_caret', 'composer_insert', 'edit'].includes(options.target)
      ? options.target
      : 'new';
    this.scientificComposerReturn = options.returnState || null;
    const initialSource = String(options.initialValue || '');
    const chemistryMode = this.scientificComposerKind === 'chemistry';
    const requested = options.guidedKind || (chemistryMode ? 'chemistry' : null);
    const eligibility = initialSource.trim() === ''
      ? null
      : guidedScientificEligibility(initialSource, chemistryMode);
    if (requested) {
      const [guidedKind, variant] = resolveGuidedStructure(requested, options.guidedVariant || '');
      const canRepresentInitial = Boolean(eligibility?.guidedAvailable && eligibility.kind === guidedKind);
      this.scientificComposerGuidedKind = canRepresentInitial ? eligibility.kind : guidedKind;
      this.scientificComposerGuidedData = canRepresentInitial
        ? eligibility.data
        : defaultGuidedScientificData(guidedKind, variant);
      this.scientificComposerInitialValue = initialSource.trim() === ''
        ? guidedScientificSource(guidedKind, this.scientificComposerGuidedData)
        : initialSource;
      const requestedView = options.viewMode || 'guided';
      this.scientificComposerViewMode = requestedView === 'guided' && (initialSource.trim() === '' || canRepresentInitial)
        ? 'guided'
        : 'source';
    } else {
      const parsed = eligibility || guidedScientificEligibility(initialSource, chemistryMode);
      this.scientificComposerGuidedKind = parsed.kind;
      this.scientificComposerGuidedData = parsed.data;
      this.scientificComposerInitialValue = initialSource;
      const requestedView = options.viewMode || (parsed.guidedAvailable ? 'guided' : 'source');
      this.scientificComposerViewMode = requestedView === 'guided' && parsed.guidedAvailable ? 'guided' : 'source';
    }
    this.renderInsertMenu();
    return true;
  }

  openScientificComposerFromPaste(kind, originalText, initialValue = originalText, displayMode = null) {
    this.preservedSelection = captureSelection(this.view.state.selection);
    this.dismissedSlashPosition = null;
    this.slashContext = null;
    this.insertMenuSource = 'button';
    this.insertMenuQuery = '';
    this.insertMenuIndex = 0;
    document.getElementById(INSERT_TRIGGER_ID)?.setAttribute('aria-expanded', 'true');
    return this.openScientificComposer(kind, {
      initialValue,
      originalText,
      displayMode: displayMode ?? (kind === 'chemistry' || String(initialValue || '').includes('\n')),
      smartPaste: true,
    });
  }

  restoreParentScientificComposer() {
    const state = this.scientificComposerReturn;
    if (!state) return false;
    this.scientificComposerKind = state.kind;
    this.scientificComposerDisplay = state.displayMode;
    this.scientificComposerInitialValue = state.source;
    this.scientificComposerOriginalText = state.originalText;
    this.scientificComposerSmartPaste = state.smartPaste;
    this.scientificComposerSelectionStart = state.selectionStart;
    this.scientificComposerSelectionEnd = state.selectionEnd;
    this.scientificComposerViewMode = state.viewMode;
    this.scientificComposerGuidedKind = state.guidedKind;
    this.scientificComposerGuidedData = cloneGuidedData(state.guidedData);
    this.scientificComposerTarget = state.target;
    this.scientificComposerReturn = null;
    this.renderInsertMenu();
    return true;
  }

  cancelScientificComposer() {
    if (this.scientificComposerKind === null) return false;
    if (this.scientificComposerSmartPaste) {
      if (!restoreSelection(this.view, this.preservedSelection)) return false;
      const plainText = this.scientificComposerOriginalText;
      this.closeInsertMenu(false);
      this.view.dispatch(this.view.state.tr.insertText(plainText).scrollIntoView());
      focusWithoutScroll(this.view);
      return true;
    }
    if (this.restoreParentScientificComposer()) return true;

    const returnToScientificTools = this.insertMenuSource === 'scientific'
      && this.scientificToolsAnchor?.isConnected
      && restoreSelection(this.view, this.preservedSelection);
    if (returnToScientificTools) {
      this.scientificComposerKind = null;
      this.scientificComposerReturn = null;
      this.scientificComposerSmartPaste = false;
      this.scientificToolsOpen = true;
      this.renderInsertMenu();
      window.requestAnimationFrame(() => this.focusScientificToolsItem(0));
      return true;
    }

    const dismissSlash = this.insertMenuSource === 'slash';
    this.closeInsertMenu(dismissSlash);
    restoreSelection(this.view, this.preservedSelection);
    focusWithoutScroll(this.view);
    this.refreshSurface();
    return true;
  }

  cancelScientificPreview() {
    this.scientificPreviewGeneration += 1;
    if (this.scientificPreviewTimer !== null) {
      window.clearTimeout(this.scientificPreviewTimer);
      this.scientificPreviewTimer = null;
    }
    if (this.scientificPreviewFrame !== null) {
      window.cancelAnimationFrame(this.scientificPreviewFrame);
      this.scientificPreviewFrame = null;
    }
  }

  scheduleScientificPreview(target, source, displayMode, immediate = false) {
    this.cancelScientificPreview();
    const generation = this.scientificPreviewGeneration;
    const render = () => {
      this.scientificPreviewFrame = null;
      if (
        this.destroyed
        || generation !== this.scientificPreviewGeneration
        || !target?.isConnected
      ) return;
      renderScientificSource(target, source, displayMode);
      this.scientificPreviewFrame = window.requestAnimationFrame(() => {
        this.scientificPreviewFrame = null;
        if (!this.destroyed && generation === this.scientificPreviewGeneration && target.isConnected) {
          this.positionInsertMenu();
        }
      });
    };
    const enqueue = () => {
      this.scientificPreviewTimer = null;
      this.scientificPreviewFrame = window.requestAnimationFrame(render);
    };
    if (immediate) enqueue();
    else this.scientificPreviewTimer = window.setTimeout(enqueue, SCIENTIFIC_PREVIEW_DELAY_MS);
  }

  renderScientificComposer() {
    if (!this.insertMenu || this.scientificComposerKind === null) return;
    this.cancelScientificPreview();
    const chemistry = this.scientificComposerKind === 'chemistry';
    const smartPaste = this.scientificComposerSmartPaste;
    this.insertMenu.dataset.surface = 'scientific-composer';
    this.insertMenu.replaceChildren();

    const header = document.createElement('div');
    header.className = 'insert-menu-header';
    const title = document.createElement('strong');
    title.id = SCIENTIFIC_COMPOSER_TITLE_ID;
    const structureLabel = GUIDED_STRUCTURE_LABELS[this.scientificComposerGuidedKind] || 'Equation';
    const insertionMode = scientificComposerInsertionMode(this.scientificComposerTarget, this.scientificComposerReturn);
    if (this.scientificComposerTarget === 'edit') title.textContent = `Edit ${structureLabel.toLowerCase()}`;
    else if (insertionMode === 'append') title.textContent = `Add ${structureLabel.toLowerCase()}`;
    else title.textContent = chemistry ? 'Insert chemical equation' : `Insert ${structureLabel.toLowerCase()}`;
    const hint = document.createElement('span');
    hint.id = SCIENTIFIC_COMPOSER_DESCRIPTION_ID;
    hint.textContent = smartPaste
      ? 'This pasted content looks scientific'
      : this.scientificComposerViewMode === 'guided'
        ? 'Fill the fields; Rustnote writes the source'
        : 'Edit unrestricted LaTeX or mhchem source';
    header.append(title, hint);
    this.insertMenu.appendChild(header);
    document.getElementById(INSERT_TRIGGER_ID)?.setAttribute('aria-haspopup', 'dialog');
    configureFloatingSurface(this.insertMenu, {
      role: 'dialog',
      labelledBy: SCIENTIFIC_COMPOSER_TITLE_ID,
      describedBy: SCIENTIFIC_COMPOSER_DESCRIPTION_ID,
      modal: false,
    });

    const body = document.createElement('div');
    body.className = 'scientific-composer scientific-composer-unified';
    const viewTabs = document.createElement('div');
    viewTabs.className = 'scientific-composer-view-tabs';
    const guidedTab = document.createElement('button');
    guidedTab.type = 'button';
    guidedTab.textContent = 'Guided';
    guidedTab.setAttribute('aria-label', 'Edit with guided scientific fields');
    guidedTab.classList.toggle('active', this.scientificComposerViewMode === 'guided');
    guidedTab.setAttribute('aria-pressed', this.scientificComposerViewMode === 'guided' ? 'true' : 'false');
    const sourceTab = document.createElement('button');
    sourceTab.type = 'button';
    sourceTab.textContent = 'Advanced source';
    sourceTab.setAttribute('aria-label', 'Edit raw scientific source');
    sourceTab.classList.toggle('active', this.scientificComposerViewMode === 'source');
    sourceTab.setAttribute('aria-pressed', this.scientificComposerViewMode === 'source' ? 'true' : 'false');
    for (const tab of [guidedTab, sourceTab]) tab.addEventListener('pointerdown', event => event.preventDefault());
    viewTabs.append(guidedTab, sourceTab);
    body.appendChild(viewTabs);

    let activeGuidedInput = null;
    let activeGuidedPath = null;
    let sourceInput = null;
    const previewLabel = document.createElement('span');
    previewLabel.className = 'scientific-composer-preview-label';
    previewLabel.textContent = 'Preview';
    const preview = document.createElement('div');
    preview.className = 'scientific-composer-preview';
    preview.setAttribute('aria-live', 'polite');

    const mode = document.createElement('div');
    mode.className = 'scientific-composer-mode';
    const locksEquationType = this.scientificComposerTarget !== 'new';
    let inline = null;
    let display = null;
    if (locksEquationType) {
      mode.dataset.locked = 'true';
      const fixedType = document.createElement('span');
      fixedType.className = 'scientific-composer-fixed-type';
      const equationType = this.scientificComposerDisplay ? 'display equation' : 'inline equation';
      fixedType.textContent = this.scientificComposerTarget === 'edit'
        ? `${equationType[0].toUpperCase()}${equationType.slice(1)}`
        : `Within ${equationType}`;
      fixedType.setAttribute(
        'aria-label',
        this.scientificComposerTarget === 'edit'
          ? `${fixedType.textContent}; use Convert from Scientific tools to change type`
          : `${fixedType.textContent}; ${insertionMode === 'append' ? 'added' : 'inserted'} notation keeps the containing equation type`,
      );
      mode.appendChild(fixedType);
    } else {
      inline = document.createElement('button');
      inline.type = 'button';
      inline.textContent = 'Inline';
      display = document.createElement('button');
      display.type = 'button';
      display.textContent = 'Display';
      for (const button of [inline, display]) button.addEventListener('pointerdown', event => event.preventDefault());
      mode.append(inline, display);
    }

    const updatePreview = (immediate = false) => {
      const source = this.scientificComposerInitialValue;
      if (inline && display) {
        inline.classList.toggle('active', !this.scientificComposerDisplay);
        display.classList.toggle('active', this.scientificComposerDisplay);
        inline.setAttribute('aria-pressed', this.scientificComposerDisplay ? 'false' : 'true');
        display.setAttribute('aria-pressed', this.scientificComposerDisplay ? 'true' : 'false');
      }
      insert.disabled = source.trim() === '';
      this.scheduleScientificPreview(preview, source, this.scientificComposerDisplay, immediate);
    };
    inline?.addEventListener('click', event => {
      event.preventDefault();
      this.scientificComposerDisplay = false;
      updatePreview();
    });
    display?.addEventListener('click', event => {
      event.preventDefault();
      this.scientificComposerDisplay = true;
      updatePreview();
    });

    const workspace = document.createElement('div');
    workspace.className = 'scientific-composer-workspace';
    if (this.scientificComposerViewMode === 'guided') {
      const guided = renderGuidedScientificForm(
        this.scientificComposerGuidedKind,
        this.scientificComposerGuidedData,
        {
          registerInput: (input, path = null) => {
            const capture = () => {
              activeGuidedInput = input;
              activeGuidedPath = Array.isArray(path) ? [...path] : null;
              this.scientificComposerSelectionStart = input.selectionStart;
              this.scientificComposerSelectionEnd = input.selectionEnd;
            };
            for (const eventName of ['focus', 'click', 'keyup', 'select']) input.addEventListener(eventName, capture);
          },
          changed: rerender => {
            this.scientificComposerInitialValue = guidedScientificSource(
              this.scientificComposerGuidedKind,
              this.scientificComposerGuidedData,
            );
            updatePreview();
            if (rerender) this.renderInsertMenu();
          },
        },
      );
      workspace.appendChild(guided);
    } else {
      sourceInput = document.createElement('textarea');
      sourceInput.className = 'scientific-composer-input scientific-composer-source-input';
      sourceInput.rows = 7;
      sourceInput.placeholder = chemistry ? '\\ce{2H2 + O2 -> 2H2O}' : '\\frac{a}{b}';
      sourceInput.setAttribute('aria-label', chemistry ? 'Chemical notation source' : 'Scientific notation source');
      sourceInput.autocapitalize = 'off';
      sourceInput.autocomplete = 'off';
      sourceInput.spellcheck = false;
      sourceInput.value = this.scientificComposerInitialValue;
      const captureSourceSelection = () => {
        this.scientificComposerSelectionStart = sourceInput.selectionStart;
        this.scientificComposerSelectionEnd = sourceInput.selectionEnd;
      };
      sourceInput.addEventListener('input', () => {
        this.scientificComposerInitialValue = sourceInput.value;
        captureSourceSelection();
        updatePreview();
      });
      for (const eventName of ['click', 'keyup', 'select']) sourceInput.addEventListener(eventName, captureSourceSelection);
      workspace.appendChild(sourceInput);
    }
    body.append(workspace, mode);

    guidedTab.addEventListener('click', event => {
      event.preventDefault();
      if (this.scientificComposerViewMode === 'guided') return;
      const eligibility = guidedScientificEligibility(this.scientificComposerInitialValue, chemistry);
      if (!eligibility.guidedAvailable) {
        this.showEditorFeedback(guidedScientificUnavailableMessage(eligibility), true);
        return;
      }
      this.scientificComposerGuidedKind = eligibility.kind;
      this.scientificComposerGuidedData = eligibility.data;
      this.scientificComposerViewMode = 'guided';
      this.renderInsertMenu();
    });
    sourceTab.addEventListener('click', event => {
      event.preventDefault();
      if (this.scientificComposerViewMode === 'source') return;
      this.scientificComposerInitialValue = guidedScientificSource(
        this.scientificComposerGuidedKind,
        this.scientificComposerGuidedData,
      );
      this.scientificComposerViewMode = 'source';
      this.scientificComposerSelectionStart = this.scientificComposerInitialValue.length;
      this.scientificComposerSelectionEnd = this.scientificComposerSelectionStart;
      this.renderInsertMenu();
    });

    const tools = document.createElement('div');
    tools.className = 'scientific-composer-tools';
    const templatesToggle = document.createElement('button');
    templatesToggle.type = 'button';
    templatesToggle.className = 'scientific-composer-tool-toggle';
    templatesToggle.textContent = chemistry ? 'Chemistry templates' : 'Templates';
    templatesToggle.setAttribute('aria-expanded', 'false');
    const symbolsToggle = document.createElement('button');
    symbolsToggle.type = 'button';
    symbolsToggle.className = 'scientific-composer-tool-toggle';
    symbolsToggle.textContent = 'More symbols…';
    symbolsToggle.setAttribute('aria-expanded', 'false');
    const templates = document.createElement('div');
    templates.className = 'scientific-composer-template-grid';
    templates.hidden = true;
    const symbols = document.createElement('div');
    symbols.className = 'scientific-composer-symbol-grid';
    symbols.hidden = true;
    const templateGroups = chemistry
      ? SCIENTIFIC_TEMPLATE_GROUPS.filter(group => group.id === 'chemistry')
      : SCIENTIFIC_TEMPLATE_GROUPS;
    const insertSource = source => {
      if (chemistry && this.scientificComposerViewMode === 'guided') {
        const eligibility = guidedScientificEligibility(source, true);
        if (eligibility.lossless) {
          this.scientificComposerGuidedKind = 'chemistry';
          this.scientificComposerGuidedData = eligibility.data;
          this.scientificComposerInitialValue = guidedScientificSource('chemistry', eligibility.data);
          this.renderInsertMenu();
          return;
        }
      }
      if (activeGuidedInput && document.contains(activeGuidedInput)) {
        insertIntoTextarea(activeGuidedInput, source);
        return;
      }
      if (sourceInput && document.contains(sourceInput)) {
        insertIntoTextarea(sourceInput, source);
        return;
      }
      const start = Number.isFinite(this.scientificComposerSelectionStart)
        ? clamp(this.scientificComposerSelectionStart, 0, this.scientificComposerInitialValue.length)
        : this.scientificComposerInitialValue.length;
      const end = Number.isFinite(this.scientificComposerSelectionEnd)
        ? clamp(this.scientificComposerSelectionEnd, start, this.scientificComposerInitialValue.length)
        : start;
      this.scientificComposerInitialValue = `${this.scientificComposerInitialValue.slice(0, start)}${source}${this.scientificComposerInitialValue.slice(end)}`;
      this.scientificComposerSelectionStart = start + scientificTemplateCaretOffset(source);
      this.scientificComposerSelectionEnd = this.scientificComposerSelectionStart;
      this.scientificComposerViewMode = 'source';
      this.renderInsertMenu();
    };
    const openGuided = kind => {
      const fieldInsertion = activeGuidedInput && activeGuidedPath
        ? {
          path: [...activeGuidedPath],
          selectionStart: activeGuidedInput.selectionStart,
          selectionEnd: activeGuidedInput.selectionEnd,
        }
        : null;
      if (!fieldInsertion && sourceInput && document.contains(sourceInput)) {
        this.scientificComposerSelectionStart = sourceInput.selectionStart;
        this.scientificComposerSelectionEnd = sourceInput.selectionEnd;
      }
      return this.openGuidedStructure(kind, 'composer', fieldInsertion);
    };
    templates.appendChild(scientificToolGroupsPanel(templateGroups, insertSource, openGuided));
    symbols.appendChild(scientificToolGroupsPanel(SCIENTIFIC_SYMBOL_GROUPS, insertSource, openGuided));
    const togglePanel = (toggle, panel, otherToggle, otherPanel) => {
      const open = panel.hidden;
      panel.hidden = !open;
      toggle.setAttribute('aria-expanded', open ? 'true' : 'false');
      otherPanel.hidden = true;
      otherToggle.setAttribute('aria-expanded', 'false');
      this.positionInsertMenu();
    };
    templatesToggle.addEventListener('click', event => {
      event.preventDefault();
      togglePanel(templatesToggle, templates, symbolsToggle, symbols);
    });
    symbolsToggle.addEventListener('click', event => {
      event.preventDefault();
      togglePanel(symbolsToggle, symbols, templatesToggle, templates);
    });
    tools.append(templatesToggle, symbolsToggle, templates, symbols);
    body.appendChild(tools);

    const note = document.createElement('p');
    note.className = 'scientific-composer-help';
    note.textContent = this.scientificComposerViewMode === 'guided'
      ? 'Use Tab to move through fields. Advanced source remains available at any time.'
      : 'Raw LaTeX and mhchem remain unrestricted.';
    body.append(note, previewLabel, preview);

    const actions = document.createElement('div');
    actions.className = 'scientific-composer-actions';
    const back = document.createElement('button');
    back.type = 'button';
    back.className = 'table-grid-back';
    back.textContent = smartPaste
      ? 'Keep as text'
      : this.scientificComposerReturn
        ? 'Back'
        : this.scientificComposerTarget === 'edit'
          ? 'Cancel'
          : 'Back';
    const insert = document.createElement('button');
    insert.type = 'button';
    insert.className = 'scientific-composer-insert';
    insert.textContent = this.scientificComposerTarget === 'edit'
      ? 'Apply'
      : insertionMode === 'append'
        ? 'Add to equation'
        : insertionMode === 'caret'
          ? 'Insert at cursor'
          : smartPaste ? 'Insert as equation' : 'Insert';
    insert.disabled = this.scientificComposerInitialValue.trim() === '';

    back.addEventListener('pointerdown', event => event.preventDefault());
    back.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      this.cancelScientificComposer();
    });
    insert.addEventListener('pointerdown', event => event.preventDefault());
    insert.addEventListener('click', event => {
      event.preventDefault();
      event.stopPropagation();
      const source = this.scientificComposerInitialValue.trim();
      if (source === '') return;
      if (this.scientificComposerTarget === 'composer_insert' && this.scientificComposerReturn) {
        const state = this.scientificComposerReturn;
        let nextSource = state.source;
        let nextGuidedData = cloneGuidedData(state.guidedData);
        let nextViewMode = state.viewMode;
        if (
          state.viewMode === 'guided'
          && state.fieldInsertion
          && insertGuidedValueAtPath(
            nextGuidedData,
            state.fieldInsertion.path,
            state.fieldInsertion.selectionStart,
            state.fieldInsertion.selectionEnd,
            source,
          )
        ) {
          nextSource = guidedScientificSource(state.guidedKind, nextGuidedData);
        } else {
          const start = Number.isFinite(state.selectionStart) ? clamp(state.selectionStart, 0, state.source.length) : state.source.length;
          const end = Number.isFinite(state.selectionEnd) ? clamp(state.selectionEnd, start, state.source.length) : start;
          nextSource = `${state.source.slice(0, start)}${source}${state.source.slice(end)}`;
          nextViewMode = 'source';
        }
        this.scientificComposerKind = state.kind;
        this.scientificComposerDisplay = state.displayMode;
        this.scientificComposerInitialValue = nextSource;
        this.scientificComposerOriginalText = state.originalText;
        this.scientificComposerSmartPaste = state.smartPaste;
        this.scientificComposerSelectionStart = null;
        this.scientificComposerSelectionEnd = null;
        this.scientificComposerViewMode = nextViewMode;
        if (nextViewMode === 'guided') {
          this.scientificComposerGuidedKind = state.guidedKind;
          this.scientificComposerGuidedData = nextGuidedData;
        } else {
          const parsed = guidedScientificEligibility(nextSource, state.kind === 'chemistry');
          this.scientificComposerGuidedKind = parsed.kind;
          this.scientificComposerGuidedData = parsed.data;
        }
        this.scientificComposerTarget = state.target;
        this.scientificComposerReturn = null;
        this.renderInsertMenu();
        return;
      }
      if (this.scientificComposerTarget === 'active_append' || this.scientificComposerTarget === 'active_caret') {
        this.closeInsertMenu(false);
        if (restoreSelection(this.view, this.preservedSelection)) this.scientific('template', source);
        return;
      }
      if (this.scientificComposerTarget === 'edit') {
        if (!restoreSelection(this.view, this.preservedSelection)) return;
        this.closeInsertMenu(false);
        this.updateActiveScientificNodeSource(source);
        return;
      }
      const applied = this.runDiscreteUserAction(() => {
        const slash = this.insertMenuSource === 'slash' ? this.slashContext : null;
        this.suppressSlashMenu = true;
        if (slash) {
          let transaction = this.view.state.tr.delete(slash.from, slash.to);
          transaction = transaction.setSelection(TextSelection.near(transaction.doc.resolve(slash.from)));
          this.view.dispatch(transaction);
        } else if (!restoreSelection(this.view, this.preservedSelection)) {
          this.suppressSlashMenu = false;
          return false;
        }
        this.closeInsertMenu(false);
        const inserted = this.insertScientificNode(
          this.scientificComposerDisplay ? 'display' : 'inline',
          source,
          0,
          false,
        );
        this.suppressSlashMenu = false;
        return inserted;
      });
      if (applied) focusWithoutScroll(this.view);
    });
    actions.append(back, insert);
    body.appendChild(actions);
    this.insertMenu.appendChild(body);
    this.insertMenu.hidden = false;
    updatePreview(true);
    this.insertMenu.querySelectorAll('details').forEach(details => {
      details.addEventListener('toggle', () => this.positionInsertMenu());
    });
    window.requestAnimationFrame(() => {
      const preferred = this.scientificComposerViewMode === 'source'
        ? sourceInput
        : this.insertMenu.querySelector('.scientific-guided-form input, .scientific-guided-form textarea');
      preferred?.focus({preventScroll: true});
      if (sourceInput) {
        const start = Number.isFinite(this.scientificComposerSelectionStart)
          ? clamp(this.scientificComposerSelectionStart, 0, sourceInput.value.length)
          : sourceInput.value.length;
        const end = Number.isFinite(this.scientificComposerSelectionEnd)
          ? clamp(this.scientificComposerSelectionEnd, start, sourceInput.value.length)
          : start;
        sourceInput.setSelectionRange(start, end);
      }
      this.positionInsertMenu();
    });
  }

  updateInsertMenuActiveItem() {
    if (!this.insertMenu) return;
    const slashOwned = this.insertMenuSource === 'slash';
    const items = [...this.insertMenu.querySelectorAll('[data-insert-command]')];
    items.forEach((item, index) => {
      const active = index === this.insertMenuIndex;
      item.dataset.active = active ? 'true' : 'false';
      item.tabIndex = slashOwned ? -1 : (active ? 0 : -1);
      if (slashOwned) item.setAttribute('aria-selected', active ? 'true' : 'false');
      else item.removeAttribute('aria-selected');
      if (active) {
        item.scrollIntoView({block: 'nearest'});
        if (slashOwned) this.view.dom.setAttribute('aria-activedescendant', item.id);
      }
    });
    if (!slashOwned || items.length === 0) this.view.dom.removeAttribute('aria-activedescendant');
  }

  openInsertMenu() {
    if (this.destroyed || !this.editable) {
      return false;
    }
    if (this.insertMenuSource === 'button') {
      this.closeInsertMenu(false);
      return true;
    }
    if (!restoreSelection(this.view, this.preservedSelection)) {
      return false;
    }
    this.dismissedSlashPosition = null;
    this.slashContext = null;
    this.insertMenuSource = 'button';
    this.insertMenuQuery = '';
    this.insertMenuIndex = 0;
    this.tableGridOpen = false;
    this.scientificToolsOpen = false;
    this.scientificToolsAnchor = null;
    this.scientificComposerKind = null;
    this.scientificComposerInitialValue = '';
    this.scientificComposerOriginalText = '';
    this.scientificComposerSmartPaste = false;
    this.scientificComposerSelectionStart = null;
    this.scientificComposerSelectionEnd = null;
    this.scientificComposerViewMode = 'guided';
    this.scientificComposerGuidedKind = 'expression';
    this.scientificComposerGuidedData = defaultGuidedScientificData('expression');
    this.scientificComposerTarget = 'new';
    this.scientificComposerReturn = null;
    document.getElementById(INSERT_TRIGGER_ID)?.setAttribute('aria-expanded', 'true');
    this.renderInsertMenu();
    return true;
  }

  closeInsertMenu(dismissSlash) {
    this.cancelScientificPreview();
    if (dismissSlash && this.insertMenuSource === 'slash' && this.slashContext) {
      this.dismissedSlashPosition = this.slashContext.from;
    }
    configureFloatingSurface(this.insertMenu);
    this.insertMenuSource = null;
    this.insertMenuQuery = '';
    this.insertMenuCommands = [];
    this.insertMenuIndex = 0;
    this.tableGridOpen = false;
    this.scientificToolsOpen = false;
    this.scientificToolsAnchor = null;
    this.scientificComposerKind = null;
    this.scientificComposerInitialValue = '';
    this.scientificComposerOriginalText = '';
    this.scientificComposerSmartPaste = false;
    this.scientificComposerSelectionStart = null;
    this.scientificComposerSelectionEnd = null;
    this.scientificComposerViewMode = 'guided';
    this.scientificComposerGuidedKind = 'expression';
    this.scientificComposerGuidedData = defaultGuidedScientificData('expression');
    this.scientificComposerTarget = 'new';
    this.scientificComposerReturn = null;
    this.slashContext = null;
    if (this.insertMenu) {
      delete this.insertMenu.dataset.surface;
      this.insertMenu.hidden = true;
      this.insertMenu.replaceChildren();
    }
    const insertTrigger = document.getElementById(INSERT_TRIGGER_ID);
    insertTrigger?.setAttribute('aria-expanded', 'false');
    insertTrigger?.setAttribute('aria-haspopup', 'menu');
    this.view.dom.removeAttribute('aria-activedescendant');
    this.view.dom.removeAttribute('aria-autocomplete');
    this.view.dom.removeAttribute('aria-controls');
    this.view.dom.removeAttribute('aria-expanded');
  }

  currentSlashContext() {
    const {selection} = this.view.state;
    if (!(selection instanceof TextSelection) || !selection.empty) {
      return null;
    }
    const {$from} = selection;
    if ($from.parent.type !== schema.nodes.paragraph) {
      return null;
    }
    const prefix = $from.parent.textBetween(0, $from.parentOffset, '\n', '\n');
    if (
      !prefix.startsWith('/')
      || prefix.length > 96
      || $from.parent.textContent !== prefix
    ) {
      return null;
    }
    return {
      from: $from.start(),
      to: selection.from,
      query: prefix.slice(1),
    };
  }

  updateSlashMenu() {
    if (this.suppressSlashMenu || this.destroyed || !this.editable) {
      return;
    }
    if (this.insertMenuSource === 'button') {
      this.positionInsertMenu();
      return;
    }
    const context = this.currentSlashContext();
    if (!context) {
      if (this.insertMenuSource === 'slash') {
        this.closeInsertMenu(false);
      }
      this.dismissedSlashPosition = null;
      return;
    }
    if (this.dismissedSlashPosition === context.from) {
      if (this.insertMenuSource === 'slash') {
        this.closeInsertMenu(false);
      }
      return;
    }
    const changed = this.insertMenuSource !== 'slash'
      || this.insertMenuQuery !== context.query
      || this.slashContext?.from !== context.from
      || this.slashContext?.to !== context.to;
    this.insertMenuSource = 'slash';
    this.insertMenuQuery = context.query;
    this.slashContext = context;
    if (changed) {
      this.insertMenuIndex = 0;
      this.renderInsertMenu();
    } else {
      this.positionInsertMenu();
    }
  }

  positionInsertMenu() {
    if (!this.insertMenu || this.insertMenu.hidden || this.insertMenuSource === null) {
      return;
    }
    let anchor = null;
    if (this.insertMenuSource === 'button') {
      anchor = document.getElementById(INSERT_TRIGGER_ID)?.getBoundingClientRect() || null;
    } else if (this.insertMenuSource === 'scientific' && this.scientificToolsAnchor?.isConnected) {
      anchor = this.scientificToolsAnchor.getBoundingClientRect();
    } else if (this.slashContext) {
      try {
        anchor = this.view.coordsAtPos(this.slashContext.to);
      } catch {
        anchor = null;
      }
    }
    if (!anchor) {
      this.closeInsertMenu(false);
      return;
    }
    const menuRect = this.insertMenu.getBoundingClientRect();
    const maxLeft = Math.max(VIEWPORT_MARGIN, window.innerWidth - menuRect.width - VIEWPORT_MARGIN);
    const preferredLeft = this.insertMenuSource === 'button'
      ? anchor.right - menuRect.width
      : anchor.left;
    const left = clamp(preferredLeft, VIEWPORT_MARGIN, maxLeft);
    let top = anchor.bottom + SURFACE_GAP;
    if (top + menuRect.height > window.innerHeight - VIEWPORT_MARGIN) {
      top = anchor.top - menuRect.height - SURFACE_GAP;
    }
    const maxTop = Math.max(VIEWPORT_MARGIN, window.innerHeight - menuRect.height - VIEWPORT_MARGIN);
    this.insertMenu.style.left = `${Math.round(left)}px`;
    this.insertMenu.style.top = `${Math.round(clamp(top, VIEWPORT_MARGIN, maxTop))}px`;
  }

  handleInsertMenuKeyDown(event) {
    if (this.insertMenuSource === null) {
      return false;
    }
    if (this.scientificComposerKind !== null) {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        return this.cancelScientificComposer();
      }
      return false;
    }
    if (this.scientificToolsOpen && this.handleScientificToolsKeyDown(event)) {
      return true;
    }
    if (this.tableGridOpen) {
      if (event.key === 'Escape') {
        event.preventDefault();
        event.stopPropagation();
        return this.returnFromTableGrid();
      }
      const gridHasFocus = (
        event.target instanceof Element
        && event.target.closest('.table-grid-picker') !== null
      );
      if (!gridHasFocus) return false;
      if (event.key === 'ArrowLeft' || event.key === 'ArrowRight' || event.key === 'ArrowUp' || event.key === 'ArrowDown') {
        event.preventDefault();
        event.stopPropagation();
        if (event.key === 'ArrowLeft') this.tableGridColumns = Math.max(1, this.tableGridColumns - 1);
        if (event.key === 'ArrowRight') this.tableGridColumns = Math.min(TABLE_GRID_SIZE, this.tableGridColumns + 1);
        if (event.key === 'ArrowUp') this.tableGridRows = Math.max(1, this.tableGridRows - 1);
        if (event.key === 'ArrowDown') this.tableGridRows = Math.min(TABLE_GRID_SIZE, this.tableGridRows + 1);
        this.updateTableGridState();
        return true;
      }
      if (event.key === ' ' || event.key.toLocaleLowerCase() === 'h') {
        event.preventDefault();
        event.stopPropagation();
        this.tableGridHeader = !this.tableGridHeader;
        this.updateTableGridState();
        return true;
      }
      if (event.key === 'Enter') {
        event.preventDefault();
        event.stopPropagation();
        this.insertTableFromGrid();
        return true;
      }
      return false;
    }
    if (this.insertMenuSource === 'button') {
      return handleCommandMenuKeyDown(event, this.insertMenu, () => {
        this.closeInsertMenu(false);
        const trigger = document.getElementById(INSERT_TRIGGER_ID);
        if (trigger instanceof HTMLElement) trigger.focus({preventScroll: true});
        else focusWithoutScroll(this.view);
      });
    }
    if (event.key === 'Escape') {
      event.preventDefault();
      event.stopPropagation();
      this.closeInsertMenu(true);
      focusWithoutScroll(this.view);
      return true;
    }
    if (event.key === 'ArrowDown' || event.key === 'ArrowUp' || event.key === 'Home' || event.key === 'End') {
      event.preventDefault();
      event.stopPropagation();
      const count = this.insertMenuCommands.length;
      if (count > 0) {
        if (event.key === 'ArrowDown') {
          this.insertMenuIndex = (this.insertMenuIndex + 1) % count;
        } else if (event.key === 'ArrowUp') {
          this.insertMenuIndex = (this.insertMenuIndex - 1 + count) % count;
        } else if (event.key === 'Home') {
          this.insertMenuIndex = 0;
        } else {
          this.insertMenuIndex = count - 1;
        }
        this.updateInsertMenuActiveItem();
      }
      return true;
    }
    if (event.key === 'Enter') {
      if (this.insertMenuCommands.length === 0) {
        return false;
      }
      event.preventDefault();
      event.stopPropagation();
      this.executeInsertCommand(this.insertMenuCommands[this.insertMenuIndex].id);
      return true;
    }
    return false;
  }

  applyInsertCommandAction(action) {
    if (action === 'equation_composer') {
      return this.openScientificComposer('equation');
    }
    if (action === 'chemical_composer') {
      return this.openScientificComposer('chemistry');
    }
    if (action === 'matrix') {
      return this.openMatrixGrid('display');
    }
    if (!restoreSelection(this.view, this.preservedSelection)) {
      return false;
    }
    if (action === 'horizontal_rule') {
      const applied = insertHorizontalRule(
        this.view.state,
        transaction => this.view.dispatch(transaction),
      );
      if (applied) {
        focusWithoutScroll(this.view);
        this.refreshSurface();
      }
      return applied;
    }
    const currentBlock = selectedBlockStyle(this.view.state);
    const currentList = selectedListType(this.view.state);
    const alreadyActive = (
      (action === 'paragraph' && currentBlock === 'paragraph')
      || (action === 'heading1' && currentBlock === 'h1')
      || (action === 'heading2' && currentBlock === 'h2')
      || (action === 'heading3' && currentBlock === 'h3')
      || (action === 'quote' && currentBlock === 'quote')
      || (action === 'code' && currentBlock === 'code')
      || (action === 'bullet' && currentList === 'bullet_list')
      || (action === 'numbered' && currentList === 'ordered_list')
      || (action === 'checklist' && currentList === 'task_list')
    );
    return alreadyActive ? true : this.format(action);
  }

  executeInsertCommand(commandId) {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.executeInsertCommand(commandId));
    }
    const commands = this.insertMenuSource === 'slash' ? SLASH_COMMANDS : INSERT_COMMANDS;
    const command = commands.find(candidate => candidate.id === commandId);
    if (!command || this.destroyed) {
      return false;
    }
    if (command.action === 'table') {
      return this.openTableGrid();
    }
    if (command.action === 'matrix') {
      return this.openMatrixGrid('display');
    }
    if (command.action === 'equation_composer') {
      return this.openScientificComposer('equation');
    }
    if (command.action === 'chemical_composer') {
      return this.openScientificComposer('chemistry');
    }
    if (command.action === 'note_link') {
      const slash = this.insertMenuSource === 'slash' ? this.slashContext : null;
      if (slash) {
        this.suppressSlashMenu = true;
        try {
          let transaction = this.view.state.tr.delete(slash.from, slash.to);
          transaction = transaction.setSelection(TextSelection.near(transaction.doc.resolve(slash.from)));
          this.view.dispatch(transaction);
        } catch {
          this.suppressSlashMenu = false;
          return false;
        }
        this.suppressSlashMenu = false;
      }
      this.closeInsertMenu(false);
      return this.requestNoteLinkPicker();
    }
    if (command.action === 'link') {
      const trigger = document.getElementById('editor-link-trigger');
      if (!(trigger instanceof HTMLButtonElement) || trigger.disabled) {
        return false;
      }
      this.closeInsertMenu(false);
      trigger.click();
      return true;
    }
    if (command.action.startsWith('attachment_')) {
      const picker = command.action.slice('attachment_'.length);
      this.closeInsertMenu(false);
      return this.requestAttachmentPicker(picker);
    }
    const slash = this.insertMenuSource === 'slash' ? this.slashContext : null;
    const slashText = slash ? `/${slash.query}` : '';
    this.suppressSlashMenu = true;
    if (slash) {
      try {
        let transaction = this.view.state.tr.delete(slash.from, slash.to);
        transaction = transaction.setSelection(TextSelection.near(transaction.doc.resolve(slash.from)));
        this.view.dispatch(transaction);
      } catch {
        this.suppressSlashMenu = false;
        return false;
      }
    }
    this.closeInsertMenu(false);
    const applied = this.applyInsertCommandAction(command.action);
    if (!applied && slash && slashText !== '') {
      const position = this.view.state.selection.from;
      this.view.dispatch(this.view.state.tr.insertText(slashText, position, position));
    }
    this.suppressSlashMenu = false;
    this.refreshSurface();
    if (applied) {
      focusWithoutScroll(this.view);
    }
    return applied;
  }

  cancelSerialization() {
    if (this.serializeTimer !== null) {
      window.clearTimeout(this.serializeTimer);
      this.serializeTimer = null;
    }
    if (this.serializeIdleHandle !== null && typeof window.cancelIdleCallback === 'function') {
      window.cancelIdleCallback(this.serializeIdleHandle);
      this.serializeIdleHandle = null;
    }
  }

  runSerialization(dispatchInput = true) {
    this.cancelSerialization();
    this.dirtySince = null;
    if (activeEditorController !== this || this.destroyed) return '';
    return writeBridge(this, dispatchInput);
  }

  scheduleSerialization() {
    if (this.initialising || this.destroyed) return;
    const now = Date.now();
    if (this.dirtySince === null) this.dirtySince = now;

    this.cancelSerialization();
    const elapsed = now - this.dirtySince;
    const delay = Math.max(
      0,
      Math.min(EDITOR_SYNC_DELAY_MS, EDITOR_SYNC_MAX_DELAY_MS - elapsed),
    );
    this.serializeTimer = window.setTimeout(() => {
      this.serializeTimer = null;
      const run = () => {
        this.serializeIdleHandle = null;
        this.runSerialization(true);
      };
      if (typeof window.requestIdleCallback === 'function') {
        const remaining = Math.max(0, EDITOR_SYNC_MAX_DELAY_MS - (Date.now() - this.dirtySince));
        this.serializeIdleHandle = window.requestIdleCallback(run, {timeout: remaining});
      } else {
        run();
      }
    }, delay);
  }

  flush(dispatchInput = false) {
    this.cancelSerialization();
    this.dirtySince = null;
    return writeBridge(this, dispatchInput);
  }


  focusEditor() {
    if (this.destroyed || activeEditorController !== this) {
      return false;
    }
    focusWithoutScroll(this.view);
    return true;
  }

  promoteSelectedBoundaries() {
    const positions = selectedTextblocks(this.view.state)
      .filter(({node}) => node.type === schema.nodes.paragraph && node.attrs.boundary === true)
      .map(({position}) => position)
      .sort((left, right) => right - left);
    if (positions.length === 0) {
      return false;
    }
    let transaction = this.view.state.tr.setMeta('addToHistory', false);
    for (const position of positions) {
      const node = transaction.doc.nodeAt(position);
      if (node?.type === schema.nodes.paragraph && node.attrs.boundary === true) {
        transaction = transaction.setNodeMarkup(position, undefined, {
          ...node.attrs,
          boundary: false,
        }, node.marks);
      }
    }
    if (!transaction.docChanged) {
      return false;
    }
    this.view.dispatch(transaction);
    return true;
  }

  format(action) {
    if (this.discreteActionDepth === 0 && action !== 'undo' && action !== 'redo') {
      return this.runDiscreteUserAction(() => this.format(action));
    }
    if (!this.editable || !restoreSelection(this.view, this.preservedSelection)) {
      return false;
    }
    if ([
      'paragraph', 'heading1', 'heading2', 'heading3', 'quote', 'code',
      'bullet', 'number', 'numbered', 'check', 'checklist',
    ].includes(action)) {
      this.promoteSelectedBoundaries();
    }
    let command = null;
    switch (action) {
      case 'bold':
        command = toggleMark(schema.marks.strong);
        break;
      case 'italic':
        command = toggleMark(schema.marks.em);
        break;
      case 'underline':
        command = toggleMark(schema.marks.underline);
        break;
      case 'strikethrough':
        command = toggleMark(schema.marks.strike);
        break;
      case 'superscript':
        command = toggleMark(schema.marks.superscript);
        break;
      case 'subscript':
        command = toggleMark(schema.marks.subscript);
        break;
      case 'highlight':
        command = toggleHighlightColor(this.lastHighlightColor);
        break;
      case 'paragraph':
      case 'heading1':
      case 'heading2':
      case 'heading3':
      case 'quote':
      case 'code':
        command = commandForBlock(action, this.view.state);
        break;
      case 'bullet':
      case 'number':
      case 'numbered':
      case 'check':
      case 'checklist':
        command = commandForList(action);
        break;
      case 'outdent':
        command = liftSelectedListItems;
        break;
      case 'indent':
        command = sinkSelectedListItems;
        break;
      case 'bullet_style_disc':
        command = setBulletListMarker('disc');
        break;
      case 'bullet_style_dash':
        command = setBulletListMarker('dash');
        break;
      case 'list_style_auto':
        command = setOrderedListStyle('auto');
        break;
      case 'list_style_decimal':
        command = setOrderedListStyle('decimal');
        break;
      case 'list_style_lower_roman':
        command = setOrderedListStyle('lower-roman');
        break;
      case 'list_style_lower_alpha':
        command = setOrderedListStyle('lower-alpha');
        break;
      case 'list_restart':
        command = restartOrderedNumbering;
        break;
      case 'list_continue':
        command = continueOrderedNumbering;
        break;
      case 'horizontal_rule':
        command = insertHorizontalRule;
        break;
      case 'table':
        return this.openTableGrid();
      case 'undo':
        command = undo;
        break;
      case 'redo':
        command = redo;
        break;
      default:
        return false;
    }
    const applied = Boolean(
      command && command(this.view.state, transaction => this.view.dispatch(transaction), this.view),
    );
    if (applied) {
      focusWithoutScroll(this.view);
      this.refreshSurface();
    }
    return applied;
  }

  setTypography(kind, value) {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.setTypography(kind, value));
    }
    if (!this.editable) return false;
    if (!restoreSelection(this.view, this.preservedSelection)) {
      return false;
    }
    let command = null;
    switch (kind) {
      case 'font_family':
        command = setFontFamily(String(value));
        break;
      case 'font_size':
        command = setFontSize(value);
        break;
      case 'line_spacing':
        command = setLineSpacing(String(value));
        break;
      case 'text_alignment':
        command = setTextAlignment(String(value));
        break;
      case 'left_indent':
        command = setParagraphLayoutAttribute('leftIndent', Number(value));
        break;
      case 'right_indent':
        command = setParagraphLayoutAttribute('rightIndent', Number(value));
        break;
      case 'first_line_indent':
        command = setParagraphLayoutAttribute('firstLineIndent', Number(value));
        break;
      case 'adjust_left_indent':
        command = adjustParagraphIndent('leftIndent', Number(value));
        break;
      case 'adjust_right_indent':
        command = adjustParagraphIndent('rightIndent', Number(value));
        break;
      case 'adjust_first_line_indent':
        command = adjustParagraphIndent('firstLineIndent', Number(value));
        break;
      case 'drop_cap':
        command = toggleDropCap;
        break;
      case 'clear_paragraph_layout':
        command = clearParagraphLayout;
        break;
      case 'text_color':
        command = setTextColor(String(value));
        break;
      case 'highlight_color': {
        const color = String(value);
        if (HIGHLIGHT_COLORS.includes(color)) {
          this.lastHighlightColor = color;
        }
        command = setHighlightColor(color);
        break;
      }
      case 'clear':
        command = clearTypography;
        break;
      default:
        return false;
    }
    const applied = Boolean(
      command && command(this.view.state, transaction => this.view.dispatch(transaction), this.view),
    );
    if (applied) {
      focusWithoutScroll(this.view);
      this.refreshSurface();
    }
    return applied;
  }

  normalizedAutoFirstLineIndent(value) {
    const numeric = Number(value);
    if (!Number.isFinite(numeric)) return DEFAULT_AUTO_FIRST_LINE_INDENT;
    return Math.max(
      0,
      Math.min(specLimit('paragraph_first_line_indent_max'), Math.round(numeric)),
    );
  }

  applyNativeSpellcheckState(recheck) {
    const dom = this.view?.dom;
    if (!dom) return;
    const attributes = nativeSpellcheckAttributes(this.spellcheckEnabled);
    for (const [name, value] of Object.entries(attributes)) {
      dom.setAttribute(name, value);
    }
    dom.spellcheck = this.spellcheckEnabled;
    if (!recheck || !this.spellcheckEnabled) return;

    // Native engines may not immediately re-scan existing content when spellcheck
    // is enabled at runtime. A false -> true hint refreshes the engine without
    // touching the ProseMirror document or relying on any platform API.
    dom.spellcheck = false;
    window.requestAnimationFrame(() => {
      if (this.destroyed || !this.spellcheckEnabled) return;
      this.view.dom.spellcheck = true;
      this.view.dom.setAttribute('spellcheck', 'true');
    });
  }

  setEditorPreferences(autoIndentEnabled, autoFirstLineIndent, defaultLineSpacing, spellcheckEnabled) {
    this.autoIndentEnabled = Boolean(autoIndentEnabled);
    this.autoFirstLineIndent = this.normalizedAutoFirstLineIndent(autoFirstLineIndent);
    this.defaultLineSpacing = LINE_SPACINGS.includes(String(defaultLineSpacing || '')) ? String(defaultLineSpacing) : null;
    const nextSpellcheckEnabled = Boolean(spellcheckEnabled);
    const spellcheckChanged = this.spellcheckEnabled !== nextSpellcheckEnabled;
    this.spellcheckEnabled = nextSpellcheckEnabled;
    this.view.updateState(this.view.state);
    this.applyNativeSpellcheckState(spellcheckChanged);
    this.refreshSurface();
    return true;
  }

  adjustFontSize(delta) {
    const selected = selectedMarkAttribute(
      this.view.state,
      schema.marks.font_size,
      'size',
      'default',
    );
    let current = Number.isFinite(Number(selected)) ? Number(selected) : null;
    if (current === null) {
      const blockStyle = selectedBlockStyle(this.view.state);
      current = defaultFontSizeForBlockStyle(blockStyle);
    }
    return this.setTypography('font_size', current + Number(delta || 0));
  }


  insertScientificNode(kind, source = '', caretOffset = 0, activateSource = true) {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.insertScientificNode(kind, source, caretOffset, activateSource));
    }
    if (!restoreSelection(this.view, this.preservedSelection)) return false;
    const type = kind === 'display' ? schema.nodes.math_display : schema.nodes.math_inline;
    const node = scientificNodeText(type, source);
    const {state} = this.view;
    let transaction = state.tr;
    let position;
    if (type === schema.nodes.math_inline && state.selection.$from.parent.inlineContent) {
      const from = state.selection.from;
      transaction = transaction.replaceSelectionWith(node);
      position = from;
    } else {
      const {$from} = state.selection;
      let depth = $from.depth;
      while (depth > 0 && !$from.node(depth).isBlock) depth -= 1;
      const blockPosition = depth > 0 ? $from.before(depth) : state.selection.from;
      const block = depth > 0 ? $from.node(depth) : null;
      if (type === schema.nodes.math_inline) {
        const wrapper = schema.nodes.paragraph.create(null, node);
        const insertAt = depth > 0 ? $from.after(depth) : state.selection.to;
        transaction = transaction.insert(insertAt, wrapper);
        position = insertAt + 1;
      } else if (block?.isTextblock && block.content.size === 0) {
        transaction = transaction.replaceWith(blockPosition, blockPosition + block.nodeSize, node);
        position = blockPosition;
      } else {
        const insertAt = depth > 0 ? $from.after(depth) : state.selection.to;
        transaction = transaction.insert(insertAt, node);
        position = insertAt;
      }
    }
    if (activateSource) {
      const sourceOffset = clamp(Number(caretOffset) || 0, 0, node.content.size);
      const inside = Math.min(position + 1 + sourceOffset, transaction.doc.content.size);
      transaction = transaction.setSelection(TextSelection.create(transaction.doc, inside));
    } else {
      let after = position + node.nodeSize;
      if (type === schema.nodes.math_display && after === transaction.doc.content.size) {
        transaction = transaction.insert(after, schema.nodes.paragraph.create());
      }
      after = Math.min(after, transaction.doc.content.size);
      transaction = transaction.setSelection(scientificBoundarySelection(transaction.doc, after, 1));
    }
    this.view.dispatch(transaction.scrollIntoView());
    focusWithoutScroll(this.view);
    this.refreshSurface();
    return true;
  }

  updateActiveScientificNodeSource(source) {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.updateActiveScientificNodeSource(source));
    }
    const context = scientificNodeContext(this.view.state);
    if (!context) return false;
    const node = scientificNodeText(context.node.type, source);
    let transaction = this.view.state.tr.replaceWith(
      context.position,
      context.position + context.node.nodeSize,
      node,
    );
    let after = context.position + node.nodeSize;
    if (node.type === schema.nodes.math_display && after === transaction.doc.content.size) {
      transaction = transaction.insert(after, schema.nodes.paragraph.create());
    }
    transaction = transaction.setSelection(scientificBoundarySelection(transaction.doc, after, 1));
    this.view.dispatch(transaction.scrollIntoView());
    focusWithoutScroll(this.view);
    this.refreshSurface();
    return true;
  }

  convertActiveScientificNode() {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.convertActiveScientificNode());
    }
    const context = scientificNodeContext(this.view.state);
    if (!context) return false;
    const convertingToDisplay = context.node.type === schema.nodes.math_inline;
    const result = convertingToDisplay
      ? inlineScientificToDisplayTransaction(this.view.state, context)
      : displayScientificToInlineTransaction(this.view.state, context);
    if (!result.transaction) {
      this.showEditorFeedback(result.error || 'This equation cannot be converted in its current document context.', true);
      return false;
    }
    this.closeInsertMenu(false);
    this.view.dispatch(result.transaction.scrollIntoView());
    focusWithoutScroll(this.view);
    this.refreshSurface();
    return true;
  }

  insertScientificTemplate(source) {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.insertScientificTemplate(source));
    }
    if (!restoreSelection(this.view, this.preservedSelection)) return false;
    const context = scientificNodeContext(this.view.state);
    const text = String(source || '');
    const caretOffset = scientificTemplateCaretOffset(text);
    if (context) {
      const start = this.view.state.selection instanceof NodeSelection
        ? context.position + 1 + context.node.content.size
        : this.view.state.selection.from;
      let transaction = this.view.state.tr.insertText(text, start, start);
      const caret = clamp(start + caretOffset, 0, transaction.doc.content.size);
      transaction = transaction.setSelection(TextSelection.create(transaction.doc, caret));
      this.view.dispatch(transaction.scrollIntoView());
      focusWithoutScroll(this.view);
      this.refreshSurface();
      return true;
    }
    return false;
  }

  showEditorFeedback(message, isError = false) {
    if (this.destroyed) return;
    if (!this.feedbackElement) {
      const feedback = document.createElement('div');
      feedback.className = 'editor-feedback';
      feedback.setAttribute('role', 'status');
      feedback.setAttribute('aria-live', 'polite');
      feedback.setAttribute('aria-atomic', 'true');
      feedback.hidden = true;
      document.body.appendChild(feedback);
      this.feedbackElement = feedback;
    }
    if (this.feedbackTimer !== null) {
      window.clearTimeout(this.feedbackTimer);
    }
    this.feedbackElement.textContent = String(message || '');
    this.feedbackElement.classList.toggle('error', Boolean(isError));
    this.feedbackElement.hidden = false;
    this.feedbackTimer = window.setTimeout(() => {
      this.feedbackTimer = null;
      if (this.feedbackElement) this.feedbackElement.hidden = true;
    }, 1800);
  }

  copyScientific(mode = 'source') {
    const context = scientificNodeContext(this.view.state);
    if (!context) return false;
    const source = context.node.textContent;
    const output = mode === 'delimited' ? scientificNodeMarkdown(context.node) : source;
    const successMessage = mode === 'delimited' ? 'Equation copied with delimiters' : 'LaTeX copied';
    void copyScientificClipboard(context.node, output).then(success => {
      if (!this.destroyed) {
        this.showEditorFeedback(success ? successMessage : 'Could not copy equation', !success);
      }
    });
    return true;
  }

  scientific(action, value = '') {
    if (!this.editable && action !== 'copy_source' && action !== 'copy_delimited') return false;
    switch (action) {
      case 'template':
        return this.insertScientificTemplate(value);
      case 'copy_source':
        return this.copyScientific('source');
      case 'copy_delimited':
        return this.copyScientific('delimited');
      default:
        return false;
    }
  }

  ensureFindDocumentIndex() {
    if (this.findIndexRevision !== this.documentRevision) {
      this.findIndexRevision = this.documentRevision;
      this.findDocumentIndex = buildFindDocumentIndex(this.view.state.doc);
      this.findMatchCache.clear();
    }
    return this.findDocumentIndex;
  }

  cachedFindMatches(normalizedQuery) {
    this.ensureFindDocumentIndex();
    if (this.findMatchCache.has(normalizedQuery)) {
      const matches = this.findMatchCache.get(normalizedQuery);
      this.findMatchCache.delete(normalizedQuery);
      this.findMatchCache.set(normalizedQuery, matches);
      return matches;
    }
    const matches = findMatches(this.findDocumentIndex, normalizedQuery);
    this.findMatchCache.set(normalizedQuery, matches);
    while (this.findMatchCache.size > 8) {
      this.findMatchCache.delete(this.findMatchCache.keys().next().value);
    }
    return matches;
  }

  updateFindDecoration(match) {
    this.view.dispatch(
      this.view.state.tr
        .setMeta(findHighlightKey, {match: match || null})
        .setMeta('addToHistory', false),
    );
  }

  scrollFindMatchIntoView(match) {
    window.requestAnimationFrame(() => {
      if (this.destroyed || !match) return;
      let dom = null;
      if (match.decorationKind === 'node') {
        dom = this.view.nodeDOM(match.decorationFrom);
      } else {
        const located = this.view.domAtPos(match.from).node;
        dom = located.nodeType === Node.ELEMENT_NODE ? located : located.parentElement;
      }
      if (dom instanceof Element) dom.scrollIntoView({block: 'center', inline: 'nearest'});
    });
  }

  find(query, requestedIndex) {
    const normalizedQuery = normalizedFindQuery(query);
    if (normalizedQuery === '') {
      this.updateFindDecoration(null);
      return 0;
    }
    const matches = this.cachedFindMatches(normalizedQuery);
    if (matches.length === 0) {
      this.updateFindDecoration(null);
      return 0;
    }
    const index = Math.max(0, Number(requestedIndex) || 0) % matches.length;
    const match = matches[index];
    this.updateFindDecoration(match);
    this.scrollFindMatchIntoView(match);
    return matches.length;
  }


  captureLinkSelection() {
    let selection = this.view.state.selection;
    const existing = selectedLinkContext(this.view.state);
    if (selection.empty && existing) {
      selection = TextSelection.create(this.view.state.doc, existing.from, existing.to);
      this.view.dispatch(this.view.state.tr.setSelection(selection));
    }
    this.preservedLinkSelection = captureSelection(selection);
    if (selection.from !== selection.to) {
      this.view.dispatch(
        this.view.state.tr
          .setMeta(preservedSelectionKey, {from: selection.from, to: selection.to})
          .setMeta('addToHistory', false),
      );
    }
    return JSON.stringify({
      label: existing?.label || this.view.state.doc.textBetween(selection.from, selection.to, ' '),
      href: existing?.href || '',
    });
  }

  insertLink(url, label) {
    if (this.discreteActionDepth === 0) {
      return this.runDiscreteUserAction(() => this.insertLink(url, label));
    }
    if (!this.editable) return false;
    const href = normalizeSafeHref(url);
    if (!href) {
      return false;
    }
    const preserved = this.preservedLinkSelection || this.preservedSelection;
    if (!restoreSelection(this.view, preserved)) {
      return false;
    }
    const {from, to} = this.view.state.selection;
    const mark = schema.marks.link.create({href, title: null});
    let transaction = this.view.state.tr.setMeta(preservedSelectionKey, false);
    if (from === to) {
      const text = label.trim() || href;
      transaction = transaction.insertText(text, from, to);
      transaction = transaction.addMark(from, from + text.length, mark);
      transaction = transaction.setSelection(TextSelection.create(transaction.doc, from + text.length));
    } else {
      transaction = transaction.addMark(from, to, mark);
    }
    this.view.dispatch(transaction.scrollIntoView());
    this.preservedLinkSelection = null;
    focusWithoutScroll(this.view);
    this.refreshSurface();
    return true;
  }

  setNoteLinkRegistry(entries) {
    this.noteLinkRegistry = new Map(
      (Array.isArray(entries) ? entries : [])
        .filter(entry => entry && validNoteId(entry.note_id))
        .map(entry => [entry.note_id, {
          title: typeof entry.title === 'string' ? entry.title : '',
          state: ['active', 'archived', 'trashed', 'missing'].includes(entry.state)
            ? entry.state
            : 'missing',
        }]),
    );
    if (this.view) this.refreshNoteLinkStates();
    return true;
  }

  refreshNoteLinkStates() {
    for (const element of this.root.querySelectorAll('.prosemirror-note-link[data-note-id], .prosemirror-media-link[data-note-id]')) {
      if (!(element instanceof HTMLElement)) continue;
      const noteId = element.dataset.noteId || '';
      const target = this.noteLinkRegistry.get(noteId);
      const isMediaLink = element.classList.contains('prosemirror-media-link');
      const state = target?.state || (isMediaLink ? 'unknown' : 'missing');
      element.classList.toggle('prosemirror-note-link-missing', state === 'missing');
      element.classList.toggle('prosemirror-note-link-trashed', state === 'trashed');
      element.classList.toggle('prosemirror-note-link-archived', state === 'archived');
      const title = String(target?.title || '').trim();
      const detail = state === 'missing'
        ? 'Linked note no longer exists'
        : state === 'trashed'
          ? `Linked note in Trash${title ? `: ${title}` : ''}`
          : state === 'archived'
            ? `Archived linked note${title ? `: ${title}` : ''}`
            : state === 'unknown'
              ? 'Media timestamp link'
              : `Linked note${title ? `: ${title}` : ''}`;
      const mediaSuffix = isMediaLink && state !== 'unknown' ? ' — media timestamp' : '';
      element.title = `${detail}${mediaSuffix}`;
      element.setAttribute('aria-label', `${detail}${mediaSuffix}`);
    }
  }

  requestNoteLinkPicker() {
    if (!this.editable) return false;
    const requestId = `note_link_${this.generation}_${this.nextNoteLinkPickerRequest++}`;
    const selection = this.view.state.selection;
    let initialQuery = '';
    if (!selection.empty) {
      initialQuery = this.view.state.doc.textBetween(selection.from, selection.to, ' ', ' ')
        .trim()
        .slice(0, 200);
    }
    const pending = {bookmark: selection.getBookmark()};
    this.pendingNoteLinkPickers.set(requestId, pending);
    if (!emitNoteLinkCommand({kind: 'pick_note_link', request_id: requestId, initial_query: initialQuery})) {
      this.pendingNoteLinkPickers.delete(requestId);
      return false;
    }
    return true;
  }

  completeNoteLinkPicker(requestId, descriptor) {
    const pending = this.pendingNoteLinkPickers.get(requestId);
    this.pendingNoteLinkPickers.delete(requestId);
    if (!pending || !descriptor || !validNoteId(descriptor.note_id)) return false;
    if (pending.bookmark) {
      try {
        const selection = pending.bookmark.resolve(this.view.state.doc);
        this.view.dispatch(this.view.state.tr.setSelection(selection).setMeta('addToHistory', false));
      } catch {
        // Fall back to the current selection if the mapped range is no longer valid.
      }
    }
    const noteId = descriptor.note_id;
    const title = String(descriptor.title || '').trim() || 'Untitled note';
    const state = ['active', 'archived', 'trashed', 'missing'].includes(descriptor.state)
      ? descriptor.state
      : 'active';
    this.noteLinkRegistry.set(noteId, {title, state});
    const {from, to} = this.view.state.selection;
    const mark = schema.marks.note_link.create({noteId});
    let transaction = this.view.state.tr.setMeta(preservedSelectionKey, false);
    if (from === to) {
      transaction = transaction.insertText(title, from, to);
      transaction = transaction.removeMark(from, from + title.length, schema.marks.link);
      transaction = transaction.removeMark(from, from + title.length, schema.marks.note_link);
      transaction = transaction.addMark(from, from + title.length, mark);
      transaction = transaction.setSelection(TextSelection.create(transaction.doc, from + title.length));
    } else {
      transaction = transaction.removeMark(from, to, schema.marks.link);
      transaction = transaction.removeMark(from, to, schema.marks.note_link);
      transaction = transaction.addMark(from, to, mark);
    }
    this.view.dispatch(transaction.scrollIntoView());
    focusWithoutScroll(this.view);
    this.refreshSurface();
    return true;
  }

  failNoteLinkPicker(requestId) {
    return this.pendingNoteLinkPickers.delete(requestId);
  }

  mediaCompatibilityState(attachmentId) {
    if (!validAttachmentId(attachmentId)) return 'unsupported';
    return this.mediaCompatibilityStates.get(attachmentId) || 'unknown';
  }

  mediaCompatibilityProfile(attachmentId) {
    if (!validAttachmentId(attachmentId)) return '';
    return this.mediaCompatibilityProfiles.get(attachmentId) || '';
  }

  mediaCompatibilityProfileFailed(attachmentId, profile) {
    if (!validAttachmentId(attachmentId)) return false;
    const normalizedProfile = String(profile || '').trim();
    if (!normalizedProfile) return false;
    return this.mediaCompatibilityFailedProfiles.get(attachmentId)?.has(normalizedProfile) === true;
  }

  markMediaCompatibilityProfileFailed(attachmentId, profile) {
    if (!validAttachmentId(attachmentId)) return false;
    const normalizedProfile = String(profile || '').trim();
    if (!normalizedProfile) return false;
    let failed = this.mediaCompatibilityFailedProfiles.get(attachmentId);
    if (!failed) {
      failed = new Set();
      this.mediaCompatibilityFailedProfiles.set(attachmentId, failed);
    }
    failed.add(normalizedProfile);
    return true;
  }

  mediaCompatibilityViews(attachmentId) {
    return [...(this.attachmentViews || [])].filter(view => view?.node?.attrs?.attachmentId === attachmentId);
  }

  refreshMediaCompatibilityViews(attachmentId) {
    for (const view of this.mediaCompatibilityViews(attachmentId)) view.render();
  }

  selectedMediaCompatibilityCandidate(attachmentView) {
    const attachmentId = attachmentView?.node?.attrs?.attachmentId;
    if (!validAttachmentId(attachmentId)) return null;
    const runtime = this.attachmentRegistry.get(attachmentId) || null;
    return attachmentView.mediaCompatibilityCandidate(runtime);
  }

  probeMediaCompatibility(attachmentView) {
    const attachmentId = attachmentView?.node?.attrs?.attachmentId;
    if (!validAttachmentId(attachmentId)) return false;
    const candidate = this.selectedMediaCompatibilityCandidate(attachmentView);
    if (!candidate) {
      this.mediaCompatibilityStates.set(attachmentId, 'unsupported');
      this.mediaCompatibilityProfiles.delete(attachmentId);
      this.refreshMediaCompatibilityViews(attachmentId);
      return false;
    }
    const currentProfile = this.mediaCompatibilityProfile(attachmentId);
    const state = currentProfile === candidate.profile ? this.mediaCompatibilityState(attachmentId) : 'unknown';
    if (!['unknown', 'absent'].includes(state)) return false;
    this.mediaCompatibilityProfiles.set(attachmentId, candidate.profile);
    this.mediaCompatibilityStates.set(attachmentId, 'probing');
    emitAttachmentCommand({
      kind: 'probe_media_compatibility',
      attachment_id: attachmentId,
      profile: candidate.profile,
    });
    this.refreshMediaCompatibilityViews(attachmentId);
    return true;
  }

  prepareMediaCompatibility(attachmentView) {
    const attachmentId = attachmentView?.node?.attrs?.attachmentId;
    if (!validAttachmentId(attachmentId)) return false;
    const candidate = this.selectedMediaCompatibilityCandidate(attachmentView);
    if (!candidate) return false;
    const currentProfile = this.mediaCompatibilityProfile(attachmentId);
    if (currentProfile && currentProfile !== candidate.profile) {
      this.mediaCompatibilityStates.set(attachmentId, 'unknown');
    }
    const state = currentProfile === candidate.profile ? this.mediaCompatibilityState(attachmentId) : 'unknown';
    if (['preparing', 'ready', 'failed', 'unsupported'].includes(state)) return false;
    this.mediaCompatibilityProfiles.set(attachmentId, candidate.profile);
    this.mediaCompatibilityStates.set(attachmentId, 'preparing');
    emitAttachmentCommand({
      kind: 'prepare_media_compatibility',
      attachment_id: attachmentId,
      profile: candidate.profile,
    });
    this.refreshMediaCompatibilityViews(attachmentId);
    return true;
  }

  ensureMediaCompatibility(attachmentView) {
    const attachmentId = attachmentView?.node?.attrs?.attachmentId;
    if (!validAttachmentId(attachmentId)) return false;
    const candidate = this.selectedMediaCompatibilityCandidate(attachmentView);
    if (!candidate) return false;
    const currentProfile = this.mediaCompatibilityProfile(attachmentId);
    const state = currentProfile === candidate.profile ? this.mediaCompatibilityState(attachmentId) : 'unknown';
    if (state === 'unknown' || state === 'absent') return this.probeMediaCompatibility(attachmentView);
    if (state === 'queued' || state === 'running') return this.prepareMediaCompatibility(attachmentView);
    return state === 'probing' || state === 'preparing' || state === 'ready';
  }

  invalidateMediaCompatibility(attachmentView) {
    const attachmentId = attachmentView?.node?.attrs?.attachmentId;
    if (!validAttachmentId(attachmentId)) return false;
    const failedCandidate = attachmentView.currentMediaPreviewCandidate?.();
    const profile = failedCandidate?.profile || this.mediaCompatibilityProfile(attachmentId);
    if (!profile) return false;
    this.mediaCompatibilityProfiles.set(attachmentId, profile);
    this.mediaCompatibilityStates.set(attachmentId, 'failed');
    this.markMediaCompatibilityProfileFailed(attachmentId, profile);
    emitAttachmentCommand({
      kind: 'invalidate_media_compatibility',
      attachment_id: attachmentId,
      profile,
    });
    this.refreshMediaCompatibilityViews(attachmentId);
    return true;
  }

  completeMediaCompatibility(attachmentId, profile, status, message = '') {
    if (!validAttachmentId(attachmentId)) return false;
    const normalizedProfile = String(profile || '').trim();
    if (!normalizedProfile) return false;
    const currentProfile = this.mediaCompatibilityProfile(attachmentId);
    if (currentProfile && currentProfile !== normalizedProfile) return false;
    this.mediaCompatibilityProfiles.set(attachmentId, normalizedProfile);
    const normalizedStatus = String(status || '').trim().toLowerCase();
    if (normalizedStatus === 'ready') {
      this.mediaCompatibilityStates.set(attachmentId, 'ready');
    } else if (normalizedStatus === 'absent') {
      this.mediaCompatibilityStates.set(attachmentId, 'absent');
      const view = this.mediaCompatibilityViews(attachmentId)[0];
      if (view) this.prepareMediaCompatibility(view);
    } else if (normalizedStatus === 'queued' || normalizedStatus === 'running') {
      this.mediaCompatibilityStates.set(attachmentId, normalizedStatus);
      const view = this.mediaCompatibilityViews(attachmentId)[0];
      if (view) this.prepareMediaCompatibility(view);
    } else if (normalizedStatus === 'unsupported') {
      this.mediaCompatibilityStates.set(attachmentId, 'failed');
      this.markMediaCompatibilityProfileFailed(attachmentId, normalizedProfile);
    } else if (normalizedStatus === 'invalidated') {
      this.mediaCompatibilityStates.set(attachmentId, 'failed');
      this.markMediaCompatibilityProfileFailed(attachmentId, normalizedProfile);
    } else {
      this.mediaCompatibilityStates.set(attachmentId, 'failed');
      this.markMediaCompatibilityProfileFailed(attachmentId, normalizedProfile);
      if (message) attachmentStatus(String(message).slice(0, 400));
    }
    this.refreshMediaCompatibilityViews(attachmentId);
    if (['unsupported', 'failed', 'error'].includes(normalizedStatus)) {
      const nextView = this.mediaCompatibilityViews(attachmentId)[0];
      if (nextView) this.ensureMediaCompatibility(nextView);
    }
    return true;
  }

  removeAttachmentFromDocument(attachmentId) {
    if (!this.editable || !validAttachmentId(attachmentId)) return false;
    const matches = [];
    this.view.state.doc.descendants((node, pos) => {
      if (node.type === schema.nodes.attachment && node.attrs.attachmentId === attachmentId) {
        matches.push({ pos, nodeSize: node.nodeSize });
        return false;
      }
      return true;
    });
    if (matches.length === 0) return false;
    return this.runDiscreteUserAction(() => {
      let transaction = this.view.state.tr;
      for (let index = matches.length - 1; index >= 0; index -= 1) {
        const match = matches[index];
        transaction = transaction.delete(match.pos, match.pos + match.nodeSize);
      }
      const nextPos = Math.min(matches[0].pos, transaction.doc.content.size);
      transaction = transaction.setSelection(
        TextSelection.near(transaction.doc.resolve(nextPos), -1),
      );
      this.view.dispatch(transaction.scrollIntoView());
      return true;
    });
  }

  setAttachmentRegistry(entries) {
    this.attachmentRegistry = new Map(
      (Array.isArray(entries) ? entries : [])
        .filter(entry => entry && typeof entry.id === 'string' && entry.id.trim() !== '')
        .map(entry => [entry.id, {
          kind: canonicalAttachmentKind(entry.kind),
          available: entry.available === true,
          previewUrl: typeof entry.preview_url === 'string' ? entry.preview_url : '',
          compatiblePreviewUrl: typeof entry.compatible_preview_url === 'string' ? entry.compatible_preview_url : '',
          screenPreviewUrl: typeof entry.screen_preview_url === 'string' ? entry.screen_preview_url : '',
          fileName: typeof entry.file_name === 'string' ? entry.file_name : '',
          mimeType: typeof entry.mime_type === 'string' ? entry.mime_type : '',
          compatiblePlaybackCandidates: (Array.isArray(entry.compatible_playback_candidates) ? entry.compatible_playback_candidates : [])
            .map(candidate => ({
              profile: typeof candidate?.profile === 'string' ? candidate.profile : '',
              mimeType: typeof candidate?.mime_type === 'string' ? candidate.mime_type : '',
              previewUrl: typeof candidate?.preview_url === 'string' ? candidate.preview_url : '',
            }))
            .filter(candidate => candidate.profile && candidate.mimeType && candidate.previewUrl),
          byteSize: Number.isSafeInteger(Number(entry.byte_size)) ? Math.max(0, Number(entry.byte_size)) : 0,
          sourceAttachmentId: typeof entry.source_attachment_id === 'string' ? entry.source_attachment_id : '',
          imageEditRecipe: typeof entry.image_edit_recipe === 'string' ? entry.image_edit_recipe : '',
          imagePixelWidth: Number.isSafeInteger(Number(entry.image_pixel_width)) ? Math.max(1, Number(entry.image_pixel_width)) : null,
          imagePixelHeight: Number.isSafeInteger(Number(entry.image_pixel_height)) ? Math.max(1, Number(entry.image_pixel_height)) : null,
          imageFormat: typeof entry.image_format === 'string' ? entry.image_format : '',
          imageOrientation: Number.isSafeInteger(Number(entry.image_orientation)) ? Number(entry.image_orientation) : null,
          imageHasAlpha: typeof entry.image_has_alpha === 'boolean' ? entry.image_has_alpha : null,
          imageAnimated: typeof entry.image_animated === 'boolean' ? entry.image_animated : null,
        }]),
    );
    for (const attachmentId of [...this.mediaCompatibilityStates.keys()]) {
      if (!this.attachmentRegistry.has(attachmentId)) {
        this.mediaCompatibilityStates.delete(attachmentId);
        this.mediaCompatibilityProfiles.delete(attachmentId);
        this.mediaCompatibilityFailedProfiles.delete(attachmentId);
      }
    }
    if (this.attachmentViews) {
      for (const view of this.attachmentViews) view.render();
    }
    this.refreshAttachmentContextMenuInteractionState();
    return true;
  }

  registerAttachmentDescriptor(descriptor) {
    const attrs = attachmentNodeAttrs(descriptor);
    if (!attrs.attachmentId) return null;
    this.attachmentRegistry.set(attrs.attachmentId, {
      kind: canonicalAttachmentKind(descriptor?.kind),
      available: !descriptor || descriptor.available !== false,
      previewUrl: descriptor && typeof descriptor.preview_url === 'string' ? descriptor.preview_url : '',
      compatiblePreviewUrl: descriptor && typeof descriptor.compatible_preview_url === 'string' ? descriptor.compatible_preview_url : '',
      screenPreviewUrl: descriptor && typeof descriptor.screen_preview_url === 'string' ? descriptor.screen_preview_url : '',
      fileName: descriptor && typeof descriptor.file_name === 'string' ? descriptor.file_name : '',
      mimeType: descriptor && typeof descriptor.mime_type === 'string' ? descriptor.mime_type : '',
      compatiblePlaybackCandidates: (descriptor && Array.isArray(descriptor.compatible_playback_candidates) ? descriptor.compatible_playback_candidates : [])
        .map(candidate => ({
          profile: typeof candidate?.profile === 'string' ? candidate.profile : '',
          mimeType: typeof candidate?.mime_type === 'string' ? candidate.mime_type : '',
          previewUrl: typeof candidate?.preview_url === 'string' ? candidate.preview_url : '',
        }))
        .filter(candidate => candidate.profile && candidate.mimeType && candidate.previewUrl),
      byteSize: descriptor && Number.isSafeInteger(Number(descriptor.byte_size)) ? Math.max(0, Number(descriptor.byte_size)) : 0,
      sourceAttachmentId: descriptor && typeof descriptor.source_attachment_id === 'string' ? descriptor.source_attachment_id : '',
      imageEditRecipe: descriptor && typeof descriptor.image_edit_recipe === 'string' ? descriptor.image_edit_recipe : '',
      imagePixelWidth: descriptor && Number.isSafeInteger(Number(descriptor.image_pixel_width)) ? Math.max(1, Number(descriptor.image_pixel_width)) : null,
      imagePixelHeight: descriptor && Number.isSafeInteger(Number(descriptor.image_pixel_height)) ? Math.max(1, Number(descriptor.image_pixel_height)) : null,
      imageFormat: descriptor && typeof descriptor.image_format === 'string' ? descriptor.image_format : '',
      imageOrientation: descriptor && Number.isSafeInteger(Number(descriptor.image_orientation)) ? Number(descriptor.image_orientation) : null,
      imageHasAlpha: descriptor && typeof descriptor.image_has_alpha === 'boolean' ? descriptor.image_has_alpha : null,
      imageAnimated: descriptor && typeof descriptor.image_animated === 'boolean' ? descriptor.image_animated : null,
    });
    return attrs;
  }

  insertAttachments(descriptors) {
    if (!this.editable || !Array.isArray(descriptors) || descriptors.length === 0) return 0;
    if (!restoreSelection(this.view, this.preservedSelection)) {
      this.view.focus();
    }
    let inserted = 0;
    for (const descriptor of descriptors) {
      const attrs = this.registerAttachmentDescriptor(descriptor);
      if (!attrs) continue;
      const node = schema.nodes.attachment.create(attrs);
      const transaction = this.view.state.tr.replaceSelectionWith(node).scrollIntoView();
      this.runDiscreteUserAction(() => {
        this.view.dispatch(transaction);
        return true;
      });
      inserted += 1;
    }
    this.preservedSelection = captureSelection(this.view.state.selection);
    focusWithoutScroll(this.view);
    return inserted;
  }

  setFocusDocumentMode(kind) {
    const next = ['editor', 'pdf', 'image', 'image_edit'].includes(kind) ? kind : 'none';
    const active = next !== 'none';
    for (const target of [document.documentElement, document.body]) {
      target.classList.toggle('rustnote-focus-active', active);
      target.classList.toggle('rustnote-editor-focus-active', next === 'editor');
      target.classList.toggle('rustnote-pdf-focus-active', next === 'pdf');
      target.classList.toggle('rustnote-image-focus-active', next === 'image');
      target.classList.toggle('rustnote-image-edit-focus-active', next === 'image_edit');
    }
  }

  hasOpenFocusTransientSurface() {
    if (this.insertMenuSource !== null || this.tableActionMenuKind !== null) return true;
    if (this.linkContextMenu && !this.linkContextMenu.hidden) return true;
    if (this.attachmentContextMenu && !this.attachmentContextMenu.hidden) return true;
    if (document.querySelector('.note-picker-backdrop, .settings-backdrop, .shortcut-backdrop, .close-durability-backdrop')) return true;
    return [...document.querySelectorAll('[data-transient-surface="true"]')].some(surface => (
      surface instanceof HTMLElement
      && !surface.hidden
      && window.getComputedStyle(surface).display !== 'none'
    ));
  }

  enterEditorFocus() {
    if (this.focusMode === 'editor') return true;
    if (this.focusMode === 'pdf') this.exitPdfFocus(this.focusedPdfView, false);
    if (this.focusMode === 'image') this.exitImageFocus(this.focusedImageView, false);
    if (this.focusMode === 'image_edit') this.exitImageEditFocus(this.focusedImageEditView, false, false);
    this.focusMode = 'editor';
    this.focusedPdfView = null;
    this.pdfFocusReturnMode = 'none';
    this.focusedImageView = null;
    this.imageFocusReturnMode = 'none';
    this.setFocusDocumentMode('editor');
    window.requestAnimationFrame(() => focusWithoutScroll(this.view));
    return true;
  }

  toggleEditorFocus() {
    return this.focusMode === 'editor' ? this.exitFocusMode('editor') : this.enterEditorFocus();
  }

  togglePdfFocus(attachmentView) {
    if (!attachmentView || !attachmentView.node || !attachmentView.node.attrs || attachmentView.node.attrs.kind !== 'pdf') return false;
    if (this.focusMode === 'image_edit' && !this.exitImageEditFocus(this.focusedImageEditView, false, false)) return false;
    if (this.focusMode === 'pdf' && this.focusedPdfView === attachmentView) {
      return this.exitPdfFocus(attachmentView);
    }
    const returnMode = this.focusMode === 'editor'
      ? 'editor'
      : (this.focusMode === 'pdf'
        ? this.pdfFocusReturnMode
        : (this.focusMode === 'image' ? this.imageFocusReturnMode : 'none'));
    if (this.focusMode === 'pdf' && this.focusedPdfView) {
      this.exitPdfFocus(this.focusedPdfView, false);
    } else if (this.focusMode === 'image' && this.focusedImageView) {
      this.exitImageFocus(this.focusedImageView, false);
    } else if (this.focusMode === 'editor') {
      this.focusMode = 'none';
      this.setFocusDocumentMode('none');
    }
    this.focusMode = 'pdf';
    this.focusedPdfView = attachmentView;
    this.pdfFocusReturnMode = returnMode;
    this.setFocusDocumentMode('pdf');
    attachmentView.applyPdfFocusState(true);
    window.requestAnimationFrame(() => { if (attachmentView.pdfFrame) attachmentView.pdfFrame.focus({preventScroll: true}); });
    return true;
  }

  exitPdfFocus(attachmentView = null, restoreReturnMode = true) {
    const active = this.focusMode === 'pdf' ? this.focusedPdfView : null;
    if (!active || (attachmentView && active !== attachmentView)) return false;
    const returnMode = restoreReturnMode ? this.pdfFocusReturnMode : 'none';
    this.focusMode = 'none';
    this.focusedPdfView = null;
    this.pdfFocusReturnMode = 'none';
    active.applyPdfFocusState(false);
    this.setFocusDocumentMode('none');
    if (returnMode === 'editor' && !this.destroyed) this.enterEditorFocus();
    return true;
  }

  toggleImageFocus(attachmentView) {
    if (!attachmentView || !attachmentView.node || !attachmentView.node.attrs || attachmentView.node.attrs.kind !== 'image') return false;
    if (this.focusMode === 'image_edit' && !this.exitImageEditFocus(this.focusedImageEditView, false, false)) return false;
    const runtime = this.attachmentRegistry.get(attachmentView.node.attrs.attachmentId);
    if (!runtime || runtime.available !== true || !runtime.previewUrl) return false;
    if (this.focusMode === 'image' && this.focusedImageView === attachmentView) {
      return this.exitImageFocus(attachmentView);
    }
    const returnMode = this.focusMode === 'editor'
      ? 'editor'
      : (this.focusMode === 'image'
        ? this.imageFocusReturnMode
        : (this.focusMode === 'pdf' ? this.pdfFocusReturnMode : 'none'));
    if (this.focusMode === 'pdf' && this.focusedPdfView) {
      this.exitPdfFocus(this.focusedPdfView, false);
    } else if (this.focusMode === 'image' && this.focusedImageView) {
      this.exitImageFocus(this.focusedImageView, false);
    } else if (this.focusMode === 'editor') {
      this.focusMode = 'none';
      this.setFocusDocumentMode('none');
    }
    this.focusMode = 'image';
    this.focusedImageView = attachmentView;
    this.imageFocusReturnMode = returnMode;
    this.setFocusDocumentMode('image');
    attachmentView.setInlineImageFocusPlaceholder(true);
    attachmentView.applyImageFocusState(true);
    return true;
  }

  exitImageFocus(attachmentView = null, restoreReturnMode = true) {
    const active = this.focusMode === 'image' ? this.focusedImageView : null;
    if (!active || (attachmentView && active !== attachmentView)) return false;
    const returnMode = restoreReturnMode ? this.imageFocusReturnMode : 'none';
    this.focusMode = 'none';
    this.focusedImageView = null;
    this.imageFocusReturnMode = 'none';
    active.applyImageFocusState(false);
    active.setInlineImageFocusPlaceholder(false);
    this.setFocusDocumentMode('none');
    if (returnMode === 'editor' && !this.destroyed) this.enterEditorFocus();
    else if (!this.destroyed) window.requestAnimationFrame(() => focusWithoutScroll(this.view));
    return true;
  }

  enterImageEditFocus(attachmentView, initialTool = 'select') {
    if (!this.editable || !attachmentView || attachmentView.node?.attrs?.kind !== 'image') return false;
    if (this.focusMode === 'image_edit' && this.focusedImageEditView === attachmentView) {
      attachmentView.setImageEditTool(initialTool);
      return true;
    }
    const returnMode = this.focusMode === 'editor'
      ? 'editor'
      : (this.focusMode === 'image' ? 'image' : 'none');
    const imageReturnMode = this.focusMode === 'image' ? this.imageFocusReturnMode : 'none';
    if (!attachmentView.beginImageEditSession(initialTool)) return false;

    if (this.focusMode === 'pdf' && this.focusedPdfView) {
      this.exitPdfFocus(this.focusedPdfView, false);
    } else if (this.focusMode === 'image' && this.focusedImageView) {
      this.focusedImageView.applyImageFocusState(false);
      this.focusedImageView = null;
      this.imageFocusReturnMode = 'none';
    } else if (this.focusMode === 'editor') {
      this.focusMode = 'none';
      this.setFocusDocumentMode('none');
    } else if (this.focusMode === 'image_edit' && this.focusedImageEditView) {
      this.exitImageEditFocus(this.focusedImageEditView, false, false);
    }

    this.focusMode = 'image_edit';
    this.focusedImageEditView = attachmentView;
    this.imageEditReturnMode = returnMode;
    this.imageEditImageReturnMode = imageReturnMode;
    this.setFocusDocumentMode('image_edit');
    attachmentView.setInlineImageFocusPlaceholder(true);
    window.requestAnimationFrame(() => attachmentView.imageEditSurface?.focus({preventScroll: true}));
    return true;
  }

  exitImageEditFocus(attachmentView = null, restoreReturnMode = true, committed = false) {
    const active = this.focusMode === 'image_edit' ? this.focusedImageEditView : null;
    if (!active || (attachmentView && active !== attachmentView)) return false;
    if (active.imageEditSaving && !committed) active.cancelImageEditApply({restore: false});
    const returnMode = restoreReturnMode ? this.imageEditReturnMode : 'none';
    const imageReturnMode = restoreReturnMode ? this.imageEditImageReturnMode : 'none';
    for (const [requestId, pendingView] of this.pendingImageEdits) {
      if (pendingView === active) this.pendingImageEdits.delete(requestId);
    }
    this.focusMode = 'none';
    this.focusedImageEditView = null;
    this.imageEditReturnMode = 'none';
    this.imageEditImageReturnMode = 'none';
    active.endImageEditSession();
    this.setFocusDocumentMode('none');

    if (returnMode === 'image' && !this.destroyed) {
      const runtime = this.attachmentRegistry.get(active.node.attrs.attachmentId);
      if (runtime?.available === true && runtime.previewUrl) {
        this.focusMode = 'image';
        this.focusedImageView = active;
        this.imageFocusReturnMode = imageReturnMode;
        this.setFocusDocumentMode('image');
        active.setInlineImageFocusPlaceholder(true);
        active.applyImageFocusState(true);
        return true;
      }
    }
    active.setInlineImageFocusPlaceholder(false);
    if (returnMode === 'editor' && !this.destroyed) this.enterEditorFocus();
    else if (!this.destroyed) window.requestAnimationFrame(() => focusWithoutScroll(this.view));
    return true;
  }

  cancelPendingImageEditForView(attachmentView) {
    if (!attachmentView) return false;
    let removed = false;
    for (const [requestId, pendingView] of this.pendingImageEdits) {
      if (pendingView !== attachmentView) continue;
      this.pendingImageEdits.delete(requestId);
      removed = true;
    }
    return removed;
  }

  requestImageEditCommit(attachmentView, payload) {
    if (!this.editable || this.destroyed || this.focusMode !== 'image_edit' || this.focusedImageEditView !== attachmentView) return false;
    if (!payload || typeof payload !== 'object') return false;
    const requestId = `image_edit_${this.generation}_${this.nextImageEditRequest++}`;
    this.pendingImageEdits.set(requestId, attachmentView);
    const sent = emitAttachmentCommand({
      kind: 'commit_image_edit',
      request_id: requestId,
      attachment_id: String(payload.attachment_id || ''),
      source_attachment_id: String(payload.source_attachment_id || ''),
      file_name: String(payload.file_name || ''),
      mime_type: String(payload.mime_type || 'image/png'),
      stage_token: String(payload.stage_token || ''),
      edit_recipe: String(payload.edit_recipe || ''),
    });
    if (!sent) this.pendingImageEdits.delete(requestId);
    return sent;
  }

  completeImageEdit(requestId, descriptor) {
    const attachmentView = this.pendingImageEdits.get(requestId);
    if (!attachmentView) return false;
    this.pendingImageEdits.delete(requestId);
    if (this.destroyed || this.focusMode !== 'image_edit' || this.focusedImageEditView !== attachmentView) return false;
    return attachmentView.completeImageEditCommit(descriptor);
  }

  failImageEdit(requestId, message = '') {
    const attachmentView = this.pendingImageEdits.get(requestId);
    if (!attachmentView) return false;
    this.pendingImageEdits.delete(requestId);
    if (this.destroyed) return false;
    return attachmentView.failImageEditCommit(message);
  }

  exitFocusMode(expectedKind = null, restoreReturnMode = true) {
    if (expectedKind && this.focusMode !== expectedKind) return false;
    if (this.focusMode === 'pdf') return this.exitPdfFocus(null, restoreReturnMode);
    if (this.focusMode === 'image') return this.exitImageFocus(null, restoreReturnMode);
    if (this.focusMode === 'image_edit') return this.exitImageEditFocus(null, restoreReturnMode, false);
    if (this.focusMode !== 'editor') return false;
    this.focusMode = 'none';
    this.setFocusDocumentMode('none');
    window.requestAnimationFrame(() => focusWithoutScroll(this.view));
    return true;
  }

  requestWindowFullscreenToggle() {
    return emitAttachmentCommand({kind: 'toggle_window_fullscreen'});
  }

  requestPdfExportSave(attachmentView, requestId, token) {
    if (!attachmentView || typeof requestId !== 'string' || !requestId.startsWith('pdf_export_')
        || typeof token !== 'string' || !/^[A-Fa-f0-9]{32}$/.test(token)) return false;
    this.pendingPdfExports.set(requestId, attachmentView);
    if (!emitAttachmentCommand({
      kind: 'save_pdf_export',
      request_id: requestId,
      attachment_id: attachmentView.node.attrs.attachmentId,
      export_token: token,
    })) {
      this.pendingPdfExports.delete(requestId);
      return false;
    }
    return true;
  }

  completePdfExport(requestId, success, message = '') {
    const attachmentView = this.pendingPdfExports.get(requestId);
    this.pendingPdfExports.delete(requestId);
    if (!attachmentView) return false;
    attachmentView.completePdfExport(requestId, Boolean(success), String(message || ''));
    return true;
  }

  pendingAttachmentImportPosition(requestId) {
    let found = null;
    this.view.state.doc.descendants((node, position) => {
      if (found !== null) return false;
      if (node.type === schema.nodes.attachment_pending && node.attrs.requestId === requestId) {
        found = position;
        return false;
      }
      return true;
    });
    return found;
  }

  updatePendingAttachmentImport(requestId, attrs = {}) {
    if (this.destroyed) return false;
    const position = this.pendingAttachmentImportPosition(requestId);
    if (!Number.isFinite(position)) return false;
    const node = this.view.state.doc.nodeAt(position);
    if (node?.type !== schema.nodes.attachment_pending) return false;
    this.view.dispatch(
      this.view.state.tr
        .setNodeMarkup(position, undefined, {...node.attrs, ...attrs})
        .setMeta('addToHistory', false),
    );
    return true;
  }

  removePendingAttachmentImport(requestId) {
    if (this.destroyed) return false;
    const position = this.pendingAttachmentImportPosition(requestId);
    if (!Number.isFinite(position)) return false;
    const node = this.view.state.doc.nodeAt(position);
    if (node?.type !== schema.nodes.attachment_pending) return false;
    this.view.dispatch(
      this.view.state.tr
        .delete(position, position + node.nodeSize)
        .setMeta('addToHistory', false),
    );
    return true;
  }

  runtimeAttachmentDescriptor(attachmentId) {
    const runtime = this.attachmentRegistry.get(attachmentId);
    if (!runtime) return null;
    return {
      id: attachmentId,
      kind: runtime.kind,
      file_name: runtime.fileName,
      mime_type: runtime.mimeType || null,
      byte_size: runtime.byteSize,
      available: runtime.available,
      preview_url: runtime.previewUrl || undefined,
      compatible_preview_url: runtime.compatiblePreviewUrl || undefined,
      screen_preview_url: runtime.screenPreviewUrl || undefined,
      compatible_playback_candidates: runtime.compatiblePlaybackCandidates || [],
      source_attachment_id: runtime.sourceAttachmentId || undefined,
      image_edit_recipe: runtime.imageEditRecipe || undefined,
      image_pixel_width: runtime.imagePixelWidth || undefined,
      image_pixel_height: runtime.imagePixelHeight || undefined,
      image_format: runtime.imageFormat || undefined,
      image_orientation: runtime.imageOrientation || undefined,
      image_has_alpha: runtime.imageHasAlpha ?? undefined,
      image_animated: runtime.imageAnimated ?? undefined,
    };
  }

  replacePendingAttachmentImport(requestId, descriptors, addToHistory) {
    if (this.destroyed || !Array.isArray(descriptors) || descriptors.length === 0) return 0;
    const position = this.pendingAttachmentImportPosition(requestId);
    if (!Number.isFinite(position)) return 0;
    const pendingNode = this.view.state.doc.nodeAt(position);
    if (pendingNode?.type !== schema.nodes.attachment_pending) return 0;

    const nodes = [];
    const attachmentIds = [];
    for (const descriptor of descriptors) {
      const attachmentId = typeof descriptor?.id === 'string' ? descriptor.id : '';
      if (!attachmentId) return 0;
      const attrs = this.registerAttachmentDescriptor(descriptor);
      if (!attrs) return 0;
      nodes.push(schema.nodes.attachment.create(attrs));
      attachmentIds.push(attachmentId);
    }

    // The insertion itself is one ProseMirror transaction, so a persisted
    // intermediate state can contain either none or all of these nodes. If the
    // app crashed after that insertion was saved but before the no-history
    // anchor removal was saved, the exact finalized sequence will be directly
    // after this anchor. Recognize only that local sequence: matching an ID
    // elsewhere in the note would incorrectly suppress a legitimate embed.
    const $position = this.view.state.doc.resolve(position);
    const anchorIndex = $position.index();
    const parent = $position.parent;
    const alreadyInserted = attachmentIds.every((attachmentId, offset) => {
      const childIndex = anchorIndex + 1 + offset;
      if (childIndex >= parent.childCount) return false;
      const child = parent.child(childIndex);
      return child.type === schema.nodes.attachment && child.attrs.attachmentId === attachmentId;
    });

    // Insert finalized nodes before removing the durable anchor. The two
    // dispatches therefore have only safe persisted states: pending only,
    // pending + finalized, or finalized only. The local sequence check above
    // makes the middle state idempotently recoverable after restart.
    if (!alreadyInserted) {
      const fragment = Fragment.fromArray(nodes);
      const insertPosition = position + pendingNode.nodeSize;
      let transaction = this.view.state.tr.insert(insertPosition, fragment).scrollIntoView();
      if (!addToHistory) transaction = transaction.setMeta('addToHistory', false);
      if (addToHistory) {
        this.runDiscreteUserAction(() => {
          this.view.dispatch(transaction);
          return true;
        });
      } else {
        this.view.dispatch(transaction);
      }
    }
    this.removePendingAttachmentImport(requestId);
    return descriptors.length;
  }

  requestPendingAttachmentRecovery() {
    if (!this.hasPendingAttachmentImports()) return false;
    return emitAttachmentCommand({kind: 'recover_pending_attachments', generation: this.generation});
  }

  hasPendingAttachmentImports() {
    let found = false;
    this.view.state.doc.descendants(node => {
      if (node.type === schema.nodes.attachment_pending) {
        found = true;
        return false;
      }
      return !found;
    });
    return found;
  }

  recoverAttachmentPickerImports(recovery) {
    if (!recovery || recovery.authoritative !== true || this.destroyed) return false;
    const groups = new Map(
      (Array.isArray(recovery.groups) ? recovery.groups : [])
        .filter(group => typeof group?.request_id === 'string' && Array.isArray(group.attachment_ids))
        .map(group => [group.request_id, group.attachment_ids.filter(id => typeof id === 'string' && id)]),
    );
    const pendingIds = [];
    this.view.state.doc.descendants(node => {
      if (node.type === schema.nodes.attachment_pending) pendingIds.push(node.attrs.requestId);
    });
    for (const requestId of pendingIds) {
      const attachmentIds = groups.get(requestId);
      if (attachmentIds?.length > 0) {
        const descriptors = attachmentIds.map(id => this.runtimeAttachmentDescriptor(id));
        if (descriptors.every(Boolean)) {
          this.replacePendingAttachmentImport(requestId, descriptors, false);
        } else {
          this.updatePendingAttachmentImport(requestId, {status: 'Finishing attachment import…'});
        }
        continue;
      }
      this.removePendingAttachmentImport(requestId);
    }
    return true;
  }

  requestAttachmentPicker(picker) {
    if (!this.editable || !specDomain('attachment_kind').includes(picker)) return false;
    const requestId = newDurableRequestId('attachment');
    const pendingNode = schema.nodes.attachment_pending.create({
      requestId,
      picker,
      label: picker === 'pdf' ? 'PDF' : `${picker.charAt(0).toUpperCase()}${picker.slice(1)} attachment`,
      status: 'Choose a file…',
      nodeSchemaVersion: schema.nodes.attachment_pending.defaultAttrs.nodeSchemaVersion,
    });
    this.view.dispatch(
      this.view.state.tr
        .replaceSelectionWith(pendingNode)
        .setMeta('addToHistory', false)
        .scrollIntoView(),
    );
    if (!emitAttachmentCommand({
      kind: 'pick_attachments',
      request_id: requestId,
      picker,
      editor_update: editorEnvelope(this),
    })) {
      this.removePendingAttachmentImport(requestId);
      attachmentStatus('Could not open the attachment picker.');
      return false;
    }
    return true;
  }

  beginAttachmentPickerImport(requestId, items) {
    const normalized = Array.isArray(items) ? items : [];
    const firstName = truncateUtf8(normalized[0]?.file_name || 'Attachment', specLimit('attachment_label_max_bytes'));
    const extra = Math.max(0, normalized.length - 1);
    const label = truncateUtf8(extra > 0 ? `${firstName} +${extra} more` : firstName, specLimit('attachment_label_max_bytes'));
    return this.updatePendingAttachmentImport(requestId, {
      label,
      status: 'Preparing managed attachment…',
      items: normalized.slice(0, specLimit('attachment_items_max')).map(item => ({
        file_name: truncateUtf8(item?.file_name || 'Attachment', specLimit('attachment_label_max_bytes')),
        kind: canonicalAttachmentKind(item?.kind),
        status: 'Preparing managed attachment…',
      })),
    });
  }

  updateAttachmentPickerImport(requestId, items) {
    const normalized = Array.isArray(items) ? items : [];
    const firstName = truncateUtf8(normalized[0]?.file_name || 'Attachment', specLimit('attachment_label_max_bytes'));
    const extra = Math.max(0, normalized.length - 1);
    const label = truncateUtf8(extra > 0 ? `${firstName} +${extra} more` : firstName, specLimit('attachment_label_max_bytes'));
    return this.updatePendingAttachmentImport(requestId, {
      label,
      status: 'Ready · verifying integrity…',
      items: normalized.slice(0, specLimit('attachment_items_max')).map(item => ({
        file_name: truncateUtf8(item?.file_name || 'Attachment', specLimit('attachment_label_max_bytes')),
        kind: canonicalAttachmentKind(item?.kind),
        status: 'Ready · verifying integrity…',
        ...(typeof item?.ingest_id === 'string' && item.ingest_id
          ? {ingest_id: truncateUtf8(item.ingest_id, specLimit('attachment_ingest_id_max_bytes'))}
          : {}),
      })),
    });
  }

  completeAttachmentPicker(requestId, descriptors) {
    return this.replacePendingAttachmentImport(requestId, descriptors, true);
  }

  failAttachmentPicker(requestId) {
    return this.removePendingAttachmentImport(requestId);
  }

  queueImageImport(files, context = {}) {
    const images = Array.from(files || []).filter(isImageImportFile);
    if (!this.editable || images.length === 0) return false;
    const totalBytes = images.reduce((total, file) => total + Math.max(0, Number(file.size) || 0), 0);
    if (totalBytes > MAX_TRANSIENT_IMAGE_IMPORT_BYTES) {
      attachmentStatus('Images exceed the 64 MB browser import limit.');
      return false;
    }
    const requestId = `img_${this.generation}_${this.nextImageImportRequest++}`;
    let pending;
    if (context.mode === 'replace') {
      pending = {mode: 'replace', attachmentView: context.attachmentView || null};
    } else {
      let selection = this.view.state.selection;
      if (Number.isFinite(context.position)) {
        const position = clampDocumentPosition(this.view.state.doc, context.position);
        try {
          selection = TextSelection.create(this.view.state.doc, position);
        } catch {
          selection = this.view.state.selection;
        }
      }
      pending = {mode: 'insert', bookmark: selection.getBookmark()};
    }
    this.pendingImageImports.set(requestId, pending);
    stageImageImportFiles(images, this.imageImportStageUrl, this.noteId)
      .then(async payloads => {
        if (!emitAttachmentCommand({kind: 'import_images', request_id: requestId, images: payloads})) {
          this.pendingImageImports.delete(requestId);
          await discardStagedImageImports(payloads, this.imageImportStageUrl);
          attachmentStatus('Could not send the image to Rustnote storage.');
        }
      })
      .catch(error => {
        this.pendingImageImports.delete(requestId);
        attachmentStatus(error instanceof Error ? error.message : 'Could not import image.');
      });
    return true;
  }

  completeImageImport(requestId, descriptors) {
    const pending = this.pendingImageImports.get(requestId);
    this.pendingImageImports.delete(requestId);
    if (!pending || !Array.isArray(descriptors) || descriptors.length === 0) return 0;
    if (pending.mode === 'replace') {
      const attachmentView = pending.attachmentView;
      const descriptor = descriptors[0];
      const attrs = this.registerAttachmentDescriptor(descriptor);
      if (!(attachmentView instanceof AttachmentView) || !attrs) return 0;
      const position = attachmentView.safePosition();
      if (position === null) return 0;
      const current = this.view.state.doc.nodeAt(position);
      if (!current || current.type !== schema.nodes.attachment || current.attrs.kind !== 'image') return 0;
      const replacementAttrs = {
        ...current.attrs,
        ...attrs,
        caption: current.attrs.caption,
        alignment: current.attrs.alignment,
        widthPercent: current.attrs.widthPercent,
        sizeMode: current.attrs.sizeMode,
      };
      this.runDiscreteUserAction(() => {
        let transaction = this.view.state.tr.setNodeMarkup(position, undefined, replacementAttrs);
        transaction = transaction.setSelection(NodeSelection.create(transaction.doc, position));
        this.view.dispatch(transaction.scrollIntoView());
        return true;
      });
      return 1;
    }

    if (pending.bookmark) {
      try {
        const selection = pending.bookmark.resolve(this.view.state.doc);
        this.view.dispatch(
          this.view.state.tr
            .setSelection(selection)
            .setMeta('addToHistory', false),
        );
      } catch {
        // Fall back to the current editor selection if the mapped insertion point is no longer valid.
      }
    }
    return this.insertAttachments(descriptors);
  }

  failImageImport(requestId) {
    return this.pendingImageImports.delete(requestId);
  }

  setEditable(editable) {
    if (this.destroyed) return false;
    const next = Boolean(editable);
    if (next === this.editable) {
      return true;
    }

    if (!next && this.focusMode === 'image_edit' && this.focusedImageEditView && !this.focusedImageEditView.imageEditSaving) {
      this.exitImageEditFocus(this.focusedImageEditView, false, false);
    }
    this.editable = next;
    this.view.setProps({editable: () => this.editable});
    this.view.dom.setAttribute('aria-readonly', String(!this.editable));
    this.root.dataset.editable = String(this.editable);
    this.view.dom.querySelectorAll('.prosemirror-task-checkbox').forEach(checkbox => {
      syncTaskCheckboxInteractionState(checkbox, this.editable);
    });
    for (const attachmentView of this.attachmentViews) attachmentView.render();
    this.refreshLinkContextMenuInteractionState();
    this.refreshAttachmentContextMenuInteractionState();
    if (!this.editable) {
      this.closeInsertMenu(false);
      this.closeTableActionMenu(false);
      this.preservedLinkSelection = null;
      this.view.dom.blur();
    }
    this.refreshSurface();
    return true;
  }

  replaceEditorState(serialized, generation) {
    if (this.destroyed) return '';
    let doc;
    try {
      doc = normalizeAtomicBoundaryDocument(parseEnvelope(serialized));
    } catch (error) {
      setEditorRuntimeError(error);
      return '';
    }

    this.cancelSerialization();
    this.dirtySince = null;
    this.exitFocusMode(null, false);
    this.closeInsertMenu(false);
    this.closeLinkContextMenu(false);
    this.closeAttachmentContextMenu(false);
    this.closeTableActionMenu(false);

    const nextState = EditorState.create({doc, schema, plugins: plugins()});
    this.view.updateState(nextState);
    this.generation = Number.isSafeInteger(Number(generation))
      ? Math.max(0, Number(generation))
      : this.generation + 1;
    this.documentRevision += 1;
    this.cachedCanonicalRevision = -1;
    this.cachedCanonicalState = '';
    this.preservedSelection = captureSelection(nextState.selection);
    this.preservedLinkSelection = null;
    this.refreshSurface();
    if (this.editable) {
      focusWithoutScroll(this.view);
    }
    clearEditorRuntimeError();
    return writeBridge(this, false);
  }

  destroy(flush = true) {
    if (this.destroyed) {
      return;
    }
    if (flush) {
      this.flush(true);
    }
    this.cancelSerialization();
    this.exitFocusMode(null, false);
    this.destroyed = true;
    this.root.removeEventListener('click', this.onEditorClickBoundary);
    document.removeEventListener('pointerdown', this.onDocumentPointerDown, true);
    document.removeEventListener('click', this.onDocumentClick, true);
    document.removeEventListener('contextmenu', this.onDocumentContextMenu, true);
    document.removeEventListener('keydown', this.onDocumentKeyDown, true);
    window.removeEventListener('pagehide', this.onPageHide);
    document.removeEventListener('visibilitychange', this.onVisibilityChange);
    document.removeEventListener('scroll', this.onViewportChange, true);
    window.removeEventListener('resize', this.onViewportChange);
    document.removeEventListener('focusin', this.onDocumentFocusChange, true);
    document.removeEventListener('focusout', this.onDocumentFocusChange, true);
    if (this.tableGeometryFrame !== null) {
      window.cancelAnimationFrame(this.tableGeometryFrame);
      this.tableGeometryFrame = null;
    }
    if (this.feedbackTimer !== null) {
      window.clearTimeout(this.feedbackTimer);
      this.feedbackTimer = null;
    }
    this.closeInsertMenu(false);
    this.floatingToolbar?.remove();
    this.linkContextMenu?.remove();
    this.attachmentContextMenu?.remove();
    this.codeContextMenu?.remove();
    this.insertMenu?.remove();
    this.tableToolbar?.remove();
    this.tableLevelControl?.remove();
    this.tableRowControl?.remove();
    this.tableColumnControl?.remove();
    this.tableActionMenu?.remove();
    this.feedbackElement?.remove();
    this.floatingToolbar = null;
    this.linkContextMenu = null;
    this.linkContext = null;
    this.linkContextReturnFocus = null;
    this.attachmentContextMenu = null;
    this.attachmentContextView = null;
    this.attachmentContextReturnFocus = null;
    this.insertMenu = null;
    this.tableToolbar = null;
    this.tableLevelControl = null;
    this.tableRowControl = null;
    this.tableColumnControl = null;
    this.tableActionMenu = null;
    this.feedbackElement = null;
    this.view.destroy();
    this.root.replaceChildren();
    this.root.dataset.runtimeReady = 'false';
  }
}
export {Controller};
