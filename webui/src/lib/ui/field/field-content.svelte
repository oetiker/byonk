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
	data-slot="field-content"
	class={cn("gap-0.5 group/field-content flex flex-1 flex-col leading-snug", className)}
	{...restProps}
>
	{@render children?.()}
</div>