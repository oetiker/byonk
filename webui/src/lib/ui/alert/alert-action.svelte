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
	data-slot="alert-action"
	class={cn("absolute top-2 right-2", className)}
	{...restProps}
>
	{@render children?.()}
</div>