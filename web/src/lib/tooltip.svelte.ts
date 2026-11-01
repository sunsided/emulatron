// One tooltip for the whole page, driven by an attachment:
//   <button {@attach tooltip('Does a thing')}>…</button>
// Shown after a short hover delay or right away on keyboard focus.

import type { Attachment } from 'svelte/attachments';

class TipState {
	text = $state<string | null>(null);
	x = $state(0);
	y = $state(0);
	below = $state(false);
}

export const tip = new TipState();

export function tooltip(text: string | null | undefined): Attachment<HTMLElement | SVGElement> {
	return (el) => {
		if (!text) return;
		let timer: ReturnType<typeof setTimeout> | undefined;

		const place = () => {
			const r = el.getBoundingClientRect();
			tip.below = r.top < 70;
			tip.x = r.left + r.width / 2;
			tip.y = tip.below ? r.bottom : r.top;
			tip.text = text;
		};
		const show = () => {
			clearTimeout(timer);
			timer = setTimeout(place, 380);
		};
		const hide = () => {
			clearTimeout(timer);
			if (tip.text === text) tip.text = null;
		};

		el.addEventListener('pointerenter', show);
		el.addEventListener('pointerleave', hide);
		el.addEventListener('pointerdown', hide);
		el.addEventListener('focus', place);
		el.addEventListener('blur', hide);
		return () => {
			hide();
			el.removeEventListener('pointerenter', show);
			el.removeEventListener('pointerleave', hide);
			el.removeEventListener('pointerdown', hide);
			el.removeEventListener('focus', place);
			el.removeEventListener('blur', hide);
		};
	};
}
