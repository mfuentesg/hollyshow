import { MonitorsColumn } from "./MonitorsColumn";
import { ServiceOrderPanel } from "./ServiceOrderPanel";
import { SlideGrid } from "./SlideGrid";

/** «Culto en vivo» (docs/design/Operator.dc.html): order, slide grid and monitors. */
export function ServicePage() {
  return (
    <div className="service-page">
      <ServiceOrderPanel />
      <SlideGrid />
      <MonitorsColumn />
    </div>
  );
}
