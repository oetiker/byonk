<script>
	import { Separator } from "$lib/ui/separator/index.js";
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		...restProps
	} = $props();
</script>

<Separator
	bind:ref
	data-slot="select-separator"
	class={cn("-mx-1 my-1 h-px bg-border pointer-events-none", className)}
	{...restProps}
/>