<svelte:options namespace="svg" />

<!-- Shared filters and gradients for both faceplates. -->
<defs>
	<filter id="seg-glow" x="-50%" y="-50%" width="200%" height="200%">
		<feGaussianBlur stdDeviation="1.6" result="b" />
		<feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
	</filter>
	<filter id="led-glow" x="-150%" y="-150%" width="400%" height="400%">
		<feGaussianBlur stdDeviation="2.5" result="b" />
		<feMerge><feMergeNode in="b" /><feMergeNode in="SourceGraphic" /></feMerge>
	</filter>
	<filter id="chip-glow" x="-20%" y="-60%" width="140%" height="220%">
		<feGaussianBlur stdDeviation="5" />
	</filter>
	<linearGradient id="alu" x1="0" y1="0" x2="0" y2="1">
		<stop offset="0" stop-color="#e2e5e6" />
		<stop offset="0.5" stop-color="#b9bec1" />
		<stop offset="1" stop-color="#8f9599" />
	</linearGradient>
	<linearGradient id="leather" x1="0" y1="0" x2="1" y2="1">
		<stop offset="0" stop-color="#714330" />
		<stop offset="1" stop-color="#4f2c1e" />
	</linearGradient>
	<pattern id="grain" width="6" height="6" patternUnits="userSpaceOnUse">
		<rect width="6" height="6" fill="transparent" />
		<circle cx="1" cy="2" r="0.6" fill="rgb(0 0 0 / 0.12)" />
		<circle cx="4" cy="5" r="0.5" fill="rgb(255 255 255 / 0.05)" />
	</pattern>
	<linearGradient id="glass" x1="0" y1="0" x2="0" y2="1">
		<stop offset="0" stop-color="#151816" />
		<stop offset="1" stop-color="#070807" />
	</linearGradient>
</defs>
