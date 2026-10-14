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
	data-slot="field-title"
	class={cn("gap-2 leading-snug text-sm font-medium group-data-[disabled=true]/field:opacity-50 flex w-fit items-center", className)}
	{...restProps}
>
	{@render children?.()}
</div>