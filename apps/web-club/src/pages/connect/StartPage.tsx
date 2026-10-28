import ConnectNav from '../../components/connect/ConnectNav';
import ConnectFooter from '../../components/connect/ConnectFooter';
import Hero, { HeroDetails } from '../../components/connect/landing/Hero';
import Ticker from '../../components/connect/landing/Ticker';
import Benefits from '../../components/connect/landing/Benefits';
import RevenueSection from '../../components/connect/landing/RevenueSection';
import PanelSection from '../../components/connect/landing/PanelSection';
import StepsSection from '../../components/connect/landing/StepsSection';
import StatsSection from '../../components/connect/landing/StatsSection';
import FaqSection from '../../components/connect/landing/FaqSection';
import FinalCta from '../../components/connect/landing/FinalCta';

/** Public presentation for club owners, framed like a focused product landing. */
export default function StartPage() {
  return (
    <div className="connect-root connect-sprint-page min-h-screen bg-drop-bg">
      <div className="connect-sprint-frame">
        <div className="connect-linear-main">
          <ConnectNav variant="landing" />
          <main>
            <Hero />
            <PanelSection />
            <HeroDetails />
            <Ticker />
            <Benefits />
            <RevenueSection />
            <StepsSection />
            <StatsSection />
            <FaqSection />
            <FinalCta />
          </main>
          <ConnectFooter />
        </div>
      </div>
    </div>
  );
}
