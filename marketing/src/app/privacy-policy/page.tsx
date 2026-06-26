import React from 'react';

export const metadata = {
  title: 'Privacy Policy | Zaps',
  description: 'Privacy Policy for Zaps',
};

export default function PrivacyPolicyPage() {
  return (
    <div className="max-w-4xl mx-auto px-4 py-12">
      <h1 className="text-4xl font-bold mb-8">Privacy Policy</h1>
      <div className="prose prose-invert max-w-none">
        <p className="text-gray-400 mb-8"><strong>Last Updated: [Date]</strong></p>

        <h2 className="text-2xl font-semibold mt-8 mb-4">1. Introduction</h2>
        <p className="mb-4">Welcome to Zaps ("we," "our," or "us"). We respect your privacy and are committed to protecting your personal data. This Privacy Policy will inform you as to how we look after your personal data when you visit our website or use our application (regardless of where you visit it from) and tell you about your privacy rights and how the law protects you.</p>

        <h2 className="text-2xl font-semibold mt-8 mb-4">2. Data We Collect</h2>
        <p className="mb-4">We may collect, use, store, and transfer different kinds of personal data about you:</p>
        <ul className="list-disc pl-6 mb-4 space-y-2">
          <li><strong>Identity Data</strong>: First name, last name, username or similar identifier.</li>
          <li><strong>Contact Data</strong>: Billing address, delivery address, email address, and telephone numbers.</li>
          <li><strong>Financial Data</strong>: Bank account, payment card details, and cryptocurrency wallet addresses.</li>
          <li><strong>Transaction Data</strong>: Details about payments to and from you.</li>
          <li><strong>Technical Data</strong>: Internet protocol (IP) address, login data, browser type and version, time zone setting and location.</li>
        </ul>

        <h2 className="text-2xl font-semibold mt-8 mb-4">3. How We Use Your Data</h2>
        <p className="mb-4">We will only use your personal data when the law allows us to. Most commonly, we will use your personal data in the following circumstances:</p>
        <ul className="list-disc pl-6 mb-4 space-y-2">
          <li>Where we need to perform the contract we are about to enter into or have entered into with you.</li>
          <li>Where it is necessary for our legitimate interests (or those of a third party).</li>
          <li>Where we need to comply with a legal or regulatory obligation (such as KYC/AML laws).</li>
        </ul>

        <h2 className="text-2xl font-semibold mt-8 mb-4">4. Your Rights (GDPR & CCPA)</h2>
        <p className="mb-4">Under certain circumstances, you have rights under data protection laws in relation to your personal data, including the right to:</p>
        <ul className="list-disc pl-6 mb-4 space-y-2">
          <li>Request access to your personal data.</li>
          <li>Request correction of your personal data.</li>
          <li>Request erasure of your personal data.</li>
          <li>Object to processing of your personal data.</li>
          <li>Request restriction of processing your personal data.</li>
          <li>Request transfer of your personal data.</li>
        </ul>

        <h2 className="text-2xl font-semibold mt-8 mb-4">5. Contact Us</h2>
        <p className="mb-4">For any legal inquiries or privacy concerns, please contact our Data Protection Officer at:</p>
        <ul className="list-disc pl-6 mb-4 space-y-2">
          <li><strong>Email:</strong> legal@zaps.com</li>
          <li><strong>Mailing Address:</strong> [Your Company Address]</li>
        </ul>
      </div>
    </div>
  );
}
