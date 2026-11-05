


function configureFloatingSurface(surface, options = {}) {
  if (!(surface instanceof HTMLElement)) return;
  for (const attribute of ['role', 'aria-label', 'aria-labelledby', 'aria-describedby', 'aria-modal']) {
    surface.removeAttribute(attribute);
  }
  if (options.role) surface.setAttribute('role', options.role);
  if (options.label) surface.setAttribute('aria-label', options.label);
  if (options.labelledBy) surface.setAttribute('aria-labelledby', options.labelledBy);
  if (options.describedBy) surface.setAttribute('aria-describedby', options.describedBy);
  if (options.modal !== undefined) surface.setAttribute('aria-modal', options.modal ? 'true' : 'false');
}

function enabledCommandMenuItems(surface) {
  if (!(surface instanceof HTMLElement)) return [];
  return [...surface.querySelectorAll('[role="menuitem"]')].filter(item => {
    if (
      !(item instanceof HTMLElement)
      || item.hidden
      || item.closest('[hidden]')
      || item.hasAttribute('disabled')
      || item.getAttribute('aria-disabled') === 'true'
      || (item instanceof HTMLButtonElement && item.disabled)
    ) return false;
    const details = item.closest('details');
    return !details || details.open || item === details.querySelector(':scope > summary');
  });
}

function focusCommandMenuItem(surface, requestedIndex = 0) {
  const items = enabledCommandMenuItems(surface);
  if (items.length === 0) return false;
  const index = Math.max(0, Math.min(requestedIndex, items.length - 1));
  items.forEach((item, itemIndex) => { item.tabIndex = itemIndex === index ? 0 : -1; });
  items[index].focus({preventScroll: true});
  return true;
}

function handleCommandMenuKeyDown(event, surface, onEscape) {
  if (!(surface instanceof HTMLElement) || surface.hidden) return false;
  if (event.key === 'Escape') {
    event.preventDefault();
    event.stopPropagation();
    onEscape?.();
    return true;
  }
  const supported = ['ArrowDown', 'ArrowUp', 'Home', 'End', 'Enter', ' '];
  if (!supported.includes(event.key)) return false;
  const items = enabledCommandMenuItems(surface);
  if (items.length === 0) return false;
  event.preventDefault();
  event.stopPropagation();
  const activeIndex = items.indexOf(document.activeElement);
  if (event.key === 'Enter' || event.key === ' ') {
    items[Math.max(0, activeIndex)]?.click();
    return true;
  }
  let next = activeIndex;
  if (event.key === 'ArrowDown') next = activeIndex < 0 ? 0 : (activeIndex + 1) % items.length;
  else if (event.key === 'ArrowUp') next = activeIndex < 0 ? items.length - 1 : (activeIndex - 1 + items.length) % items.length;
  else if (event.key === 'Home') next = 0;
  else if (event.key === 'End') next = items.length - 1;
  return focusCommandMenuItem(surface, next);
}
export {configureFloatingSurface, enabledCommandMenuItems, focusCommandMenuItem, handleCommandMenuKeyDown};
