<script lang="ts">
	import { LINE, type Machine } from '#lib/emulator.svelte.js';
	import SevenSeg from './SevenSeg.svelte';
	import Led from './Led.svelte';
	import SvgDefs from './SvgDefs.svelte';
	import KeyHit from './KeyHit.svelte';
	import { keyTip } from '#lib/keyhelp.js';

	let { machine }: { machine: Machine } = $props();

	const frame = $derived(machine.frame);
	const bus = $derived(machine.busLines());
	const keys = $derived(machine.info.keys);
	const groups = $derived(machine.info.groups);

	// Block diagram as printed on the panel, in the photo's own coordinates
	// (scaled down as a group). `glow` names a chip activity counter.
	const BOXES: { x: number; y: number; w: number; h: number; lines: string[]; glow?: string; dashed?: boolean; small?: boolean }[] = [
		{ x: 95, y: 25, w: 120, h: 40, lines: ['TAKT'] },
		{ x: 85, y: 95, w: 120, h: 120, lines: ['CPU', '', 'U880D'], glow: 'CPU' },
		{ x: 50, y: 230, w: 120, h: 56, lines: ['NETZTEIL', '+5 V'], small: true },
		{ x: 268, y: 20, w: 100, h: 76, lines: ['2K-', 'ROM', 'U505D'], glow: 'ROM' },
		{ x: 393, y: 20, w: 100, h: 76, lines: ['1K-', 'RAM', 'U202D'], glow: 'RAM' },
		{ x: 520, y: 20, w: 100, h: 76, lines: ['Erweiterung', 'durch', 'Anwender'], dashed: true, small: true },
		{ x: 245, y: 228, w: 96, h: 72, lines: ['PIO 1', 'U855D'], glow: 'PIO1' },
		{ x: 375, y: 228, w: 96, h: 72, lines: ['PIO 2', 'U855D'], glow: 'PIO2' },
		{ x: 505, y: 228, w: 96, h: 72, lines: ['CTC', 'U857D'], glow: 'CTC' },
		{ x: 700, y: 80, w: 110, h: 165, lines: ['BUS-', 'Analysator', 'SCHALTER', 'ANZEIGEN'], small: true }
	];
	const TAPS = [318, 443, 570];

	const KEY_FACE: Record<string, [string, string, string]> = {
		orange: ['#ec7a2c', '#b4541a', '#24140a'],
		grey: ['#d9d8d2', '#a3a29b', '#1f1f1f'],
		red: ['#cf2f24', '#8e1c15', '#fff4ee'],
		teal: ['#2fa391', '#1d6f62', '#08221d']
	};

	function keyPos(row: number, col: number, label: string): [number, number, number] {
		if (label === 'EXEC') return [693, 372 + 3 * 46, 50];
		if (col < 4) return [430 + col * 58, 372 + row * 46, 52];
		if (col < 8) return [666 + (col - 5) * 56, 372 + row * 46, 50];
		return [800, 372 + row * 46, 52];
	}

	const lineHeight = (b: { lines: string[]; small?: boolean }) => (b.small || b.lines.length >= 3 ? 20 : 26);

	function lineColor(g: number) {
		const a = [196, 190, 182];
		const b = [255, 226, 140];
		const m = (i: number) => Math.round(a[i] + (b[i] - a[i]) * Math.min(1, g));
		return `rgb(${m(0)} ${m(1)} ${m(2)})`;
	}

	// LED x positions: groups of four with a little gap.
	const ledX = (i: number, x0: number) => x0 + i * 21 + Math.floor(i / 4) * 8;
	const CONTROL = [
		['M1', LINE.M1],
		['MREQ', LINE.MREQ],
		['IORQ', LINE.IORQ],
		['RD', LINE.RD],
		['WR', LINE.WR],
		['HALT', LINE.HALT]
	] as const;
</script>

<svg viewBox="0 0 1000 640" role="img" aria-label="Poly-Computer 880 panel">
	<SvgDefs />

	<!-- Aluminium case and panel -->
	<rect x="0" y="0" width="1000" height="640" rx="18" fill="url(#alu)" />
	<rect x="20" y="20" width="960" height="600" rx="8" fill="#2a211d" />
	<rect x="26" y="26" width="948" height="588" rx="6" fill="#3d302a" />
	{#each [[48, 46], [952, 46], [48, 594], [952, 594], [610, 46], [610, 210]] as [cx, cy] (cx * 1000 + cy)}
		<circle {cx} {cy} r="4.5" fill="#9b9c98" stroke="#2a211d" />
	{/each}

	<!-- Printed block diagram, live -->
	<g transform="translate(64 44) scale(0.6)" class="diagram">
		{#each TAPS as x (x)}
			<rect x={x - 9} y="96" width="18" height="40" fill="#bdbab3" />
			<rect x={x - 9} y="182" width="18" height="46" fill="#bdbab3" />
		{/each}
		<rect x="140" y="214" width="16" height="16" fill="#bdbab3" />
		<path
			d="M205 136 H676 L700 159 L676 182 H205 Z"
			fill={lineColor(machine.glow.CPU ?? 0)}
			opacity="0.9"
		/>
		<circle cx="235" cy="159" r="11" fill="#8f8c86" />
		<circle cx="640" cy="159" r="11" fill="#8f8c86" />
		<text x="440" y="168" class="bar">BUSSYSTEM</text>
		<rect x="148" y="65" width="14" height="30" fill="#bdbab3" />
		{#each BOXES as b (b.lines.join())}
			{@const g = b.glow ? (machine.glow[b.glow] ?? 0) : 0}
			{@const c = lineColor(g)}
			<rect
				x={b.x}
				y={b.y}
				width={b.w}
				height={b.h}
				rx="6"
				fill="#3d302a"
				stroke={c}
				stroke-width="4"
				stroke-dasharray={b.dashed ? '9 7' : undefined}
			/>
			{#each b.lines as line, i (i)}
				<text
					x={b.x + b.w / 2}
					y={b.y + (b.h - b.lines.length * lineHeight(b)) / 2 + (i + 1) * lineHeight(b) - 5}
					class={['box', { small: b.small || i > 0 }]}
					fill={c}>{line}</text
				>
			{/each}
		{/each}
	</g>

	<!-- Vents -->
	{#each Array.from({ length: 8 }, (_, i) => i) as i (i)}
		<rect x={690 + i * 16} y="64" width="8" height="64" rx="3" fill="#1d1714" />
	{/each}
	{#each Array.from({ length: 6 }, (_, i) => i) as i (i)}
		<rect x={850 + i * 16} y="64" width="8" height="64" rx="3" fill="#1d1714" />
	{/each}

	<!-- Window strip: bus analyser and display -->
	<rect x="96" y="222" width="808" height="122" rx="4" fill="#9ea2a4" />
	<rect x="100" y="226" width="800" height="114" rx="2" fill="url(#glass)" />
	<g class="strip">
		<text x="114" y="242">ADRESSBUS</text>
		{#each Array.from({ length: 16 }, (_, i) => i) as i (i)}
			<text x={ledX(i, 200)} y="242" class="num">{15 - i}</text>
			<Led x={ledX(i, 200)} y={254} r={5} level={bus[15 - i]} />
		{/each}
		<text x="114" y="280">DATENBUS</text>
		{#each Array.from({ length: 8 }, (_, i) => i) as i (i)}
			<text x={ledX(i, 200)} y="280" class="num">{7 - i}</text>
			<Led x={ledX(i, 200)} y={292} r={5} level={bus[16 + 7 - i]} />
		{/each}
		<text x="410" y="296">MCYCL</text>
		<Led x={458} y={292} r={5} level={machine.cycleMode ? 1 : 0} />
		<text x="114" y="320">STEUERBUS</text>
		{#each CONTROL as [name, line], i (name)}
			<Led x={200 + i * 34} y={316} r={5} level={bus[line]} />
			<text x={200 + i * 34} y="334" class="num">{name}</text>
		{/each}
		<Led x={420} y={316} r={5} level={bus[LINE.INT]} />
		<text x="420" y="334" class="num">INT</text>
		<Led x={454} y={316} r={5} level={bus[LINE.NMI]} />
		<text x="454" y="334" class="num">NMI</text>
		<Led x={488} y={316} r={5} level={machine.cycleMode ? 1 : 0} />
		<text x="488" y="334" class="num">WAIT</text>

		{#each groups as g (g.label)}
			<text x={560 + (g.first + g.count / 2) * 41 - 6} y="242" class="num">{g.label}</text>
		{/each}
		{#each frame?.digits ?? [] as segs, i (i)}
			<SevenSeg x={562 + i * 41} y={256} scale={0.86} {segs} />
		{/each}
	</g>

	<!-- Keypad -->
	{#each keys as key, i (key.label)}
		{@const [x, y, w] = keyPos(key.row, key.col, key.label)}
		{@const down = frame?.keys_down[i] || (key.label === 'M CYCL' && machine.cycleMode)}
		{@const [face, edge, ink] = KEY_FACE[key.color] ?? KEY_FACE.grey}
		{@const lines = key.label === 'M CYCL' ? ['M', 'CYCL'] : [key.label]}
		<KeyHit {machine} index={i} label={key.label} tip={keyTip(machine.info.family, key)} x={x} y={y} w={w} h={40}>
			<rect {x} y={y + 3} width={w} height="37" rx="5" fill={edge} />
			<rect {x} y={y + (down ? 3 : 0)} width={w} height="36" rx="5" fill={down ? '#ffd76a' : face} />
			{#each lines as l, li (li)}
				<text
					x={x + w / 2}
					y={y + (down ? 3 : 0) + (key.sub ? 19 : lines.length > 1 ? 16 + li * 13 : 24)}
					class={['cap', { long: l.length > 2, hex: key.color === 'orange' }]}
					fill={ink}>{l}</text
				>
			{/each}
			{#if key.sub}
				<text x={x + w / 2} y={y + (down ? 3 : 0) + 31} class="sub" fill={ink}>{key.sub}</text>
			{/if}
		</KeyHit>
	{/each}

	<!-- Maker -->
	<g transform="translate(96 470)">
		<rect width="56" height="56" rx="3" fill="#f2efe7" />
		<circle cx="20" cy="18" r="6" fill="#3d302a" />
		<circle cx="36" cy="18" r="6" fill="#3d302a" />
		<path d="M10 46 Q20 26 28 40 Q36 26 46 46 Z" fill="#3d302a" />
		<text x="28" y="72" class="scoli">SCOLI</text>
	</g>
	<text x="96" y="584" class="brand">POLY-COMPUTER 880</text>
</svg>

<style>
	svg {
		display: block;
		width: 100%;
		height: auto;
		font-family: var(--font-label);
		user-select: none;
	}
	.diagram .box {
		font-size: 24px;
		font-weight: 700;
		text-anchor: middle;
		letter-spacing: 0.04em;
	}
	.diagram .box.small {
		font-size: 17px;
		font-weight: 600;
	}
	.diagram .bar {
		font-size: 24px;
		font-weight: 700;
		text-anchor: middle;
		fill: #3d302a;
		letter-spacing: 0.12em;
	}
	.strip text {
		font-size: 9.5px;
		fill: #c9cdc6;
		letter-spacing: 0.06em;
	}
	.strip .num {
		font-size: 8px;
		text-anchor: middle;
		fill: #9aa098;
	}
	.cap {
		font-size: 13px;
		font-weight: 600;
		text-anchor: middle;
		pointer-events: none;
	}
	.cap.hex {
		font-size: 20px;
		font-weight: 700;
	}
	.cap.long {
		font-size: 12px;
	}
	.sub {
		font-size: 8.5px;
		text-anchor: middle;
		pointer-events: none;
		opacity: 0.85;
	}
	.brand {
		font-size: 34px;
		font-weight: 500;
		fill: #f4f1ea;
		letter-spacing: 0.04em;
	}
	.scoli {
		font-size: 10px;
		fill: #f2efe7;
		text-anchor: middle;
		letter-spacing: 0.14em;
	}
</style>
