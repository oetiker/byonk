<script>
	import * as ResizablePrimitive from "paneforge";
	import { cn } from "$lib/utils.js";

	let {
		ref = $bindable(null),
		this: paneGroup = $bindable(),
		class: className,
		...restProps
	} = $props();

	// byonk: expose the group's layout API through the wrapper, so a
	// `bind:this` on Resizable.PaneGroup can read and set the layout.
	export const getLayout = () => paneGroup?.getLayout() ?? [];
	export const setLayout = (layout) => paneGroup?.setLayout(layout);
</script>

<ResizablePrimitive.PaneGroup
	bind:ref
	bind:this={paneGroup}
	data-slot="resizable-pane-group"
	class={cn(
		"flex h-full w-full data-[direction=vertical]:flex-col",
		className
	)}
	{...restProps}
/>