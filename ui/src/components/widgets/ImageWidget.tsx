import type {
	ImageConfig,
	WidgetDefinition,
	WidgetProps,
} from "@/types/widget";

const ImageWidget: React.FC<WidgetProps<ImageConfig>> = ({
	config,
	resolve,
}) => {
	const bg = resolve(config.backgroundColor ?? "var(--bg-widget)");
	const border = resolve(config.borderColor ?? "var(--border-color)");
	const image = config.image ? resolve(config.image) : undefined;
	const pad = "6px";
	const posStyles: Record<string, React.CSSProperties> = {
		t: { top: pad, left: "50%", transform: "translateX(-50%)" },
		b: { bottom: pad, left: "50%", transform: "translateX(-50%)" },
		l: { left: pad, top: "50%", transform: "translateY(-50%)" },
		r: { right: pad, top: "50%", transform: "translateY(-50%)" },
		c: { top: "50%", left: "50%", transform: "translate(-50%, -50%)" },
		tl: { top: pad, left: pad },
		tr: { top: pad, right: pad },
		bl: { bottom: pad, left: pad },
		br: { bottom: pad, right: pad },
	};
	const labelPos = posStyles[config.labelPosition ?? "c"] ?? posStyles.c;

	return (
		<div style={{ position: "relative", width: "100%", height: "100%" }}>
			{/** biome-ignore lint/a11y/useAltText: <explanation> */}
			<img
				src={image ? image : undefined}
				style={{
					backgroundColor: bg,
					backgroundSize: "cover",
					border: `1px solid ${border}`,
					borderRadius: "var(--widget-radius)",
					width: "100%",
					height: "100%",
				}}
			/>
			{config.label && (
				<div
					style={{
						position: "absolute",
						...labelPos,
						transform: "translate(-50%, -50%)",
						color: "var(--text-primary)",
						fontSize: "1rem",
						fontWeight: 500,
						pointerEvents: "none",
						textAlign: "center",
						backgroundColor: "rgba(0, 0, 0, 0.8)",
					}}
				>
					{resolve(config.label)}
				</div>
			)}
		</div>
	);
};

export const imageDefinition: WidgetDefinition = {
	type: "image",
	label: "Image",
	icon: "image",
	defaultSize: { w: 1, h: 1 },
	configSchema: [
		{
			key: "image",
			label: "Image URL",
			type: "text",
		},
	],
	component: ImageWidget,
};
