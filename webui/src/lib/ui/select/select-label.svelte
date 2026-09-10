<script>
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	} = $props();
</script>

<div
	bind:this={ref}
	data-slot="select-label"
	class={cn("px-1.5 py-1 text-xs text-muted-foreground", className)}
	{...restProps}
>
	{@render children?.()}
</div>