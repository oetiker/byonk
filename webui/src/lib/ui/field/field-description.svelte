<script>
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	} = $props();
</script>

<p
	bind:this={ref}
	data-slot="field-description"
	class={cn(
		"text-muted-foreground text-left text-sm [[data-variant=legend]+&]:-mt-1.5 leading-normal font-normal group-has-data-horizontal/field:text-balance",
		"last:mt-0 nth-last-2:-mt-1",
		"[&>a]:underline [&>a]:underline-offset-4 [&>a:hover]:text-primary",
		className
	)}
	{...restProps}
>
	{@render children?.()}
</p>