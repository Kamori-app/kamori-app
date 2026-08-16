/**
 * @typedef {{ id: string, absTop: number }} SectionPosition
 */

/**
 * Returns sticky-nav aware offset used to determine which section is active.
 * @param {number} navHeight
 * @returns {number}
 */
export function computeTopOffset(navHeight) {
  return Math.max(96, Math.round(navHeight + 24));
}

/**
 * Picks active section based on current scroll position and section absolute tops.
 * @param {SectionPosition[]} sections
 * @param {number} scrollY
 * @param {number} topOffset
 * @returns {string | null}
 */
export function pickActiveSection(sections, scrollY, topOffset) {
  if (sections.length === 0) {
    return null;
  }

  const anchor = scrollY + topOffset + 1;
  let current = sections[0].id;
  for (const section of sections) {
    if (section.absTop <= anchor) {
      current = section.id;
    }
  }
  return current;
}

/**
 * Computes final scroll target when user clicks section in sticky navigation.
 * @param {number} sectionAbsTop
 * @param {number} navHeight
 * @param {number} [extraOffset=20]
 * @returns {number}
 */
export function computeScrollTarget(sectionAbsTop, navHeight, extraOffset = 20) {
  return sectionAbsTop - (navHeight + extraOffset);
}
