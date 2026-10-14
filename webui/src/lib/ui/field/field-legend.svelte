<script>
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		variant = "legend",
		children,
		...restProps
	} = $props();
</script>

<legend
	bind:this={ref}
	data-slot="field-legend"
	data-variant={variant}
	class={cn("mb-1.5 font-medium data-[variant=label]:text-sm data-[variant=legend]:text-base", className)}
	{...restProps}
>
	{@render children?.()}
</legend>