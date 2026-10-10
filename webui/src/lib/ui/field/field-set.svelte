<script>
	import { cn } from "$lib/utils.js";
	let {
		ref = $bindable(null),
		class: className,
		children,
		...restProps
	} = $props();
</script>

<fieldset
	bind:this={ref}
	data-slot="field-set"
	class={cn("gap-4 has-[>[data-slot=checkbox-group]]:gap-3 has-[>[data-slot=radio-group]]:gap-3 flex flex-col", className)}
	{...restProps}
>
	{@render children?.()}
</fieldset>