"use client";

import { useState } from 'react';

interface FederationSelectorProps {
  onSelect: (federation: string) => void;
  federations?: string[];
  initialFederation?: string;
}

export function FederationSelector({ 
  onSelect,
  federations = ['global', 'federation1', 'federation2'],
  initialFederation = 'global'
}: FederationSelectorProps) {
  const [selected, setSelected] = useState(initialFederation);
  
  const handleSelect = (federation: string) => {
    setSelected(federation);
    onSelect(federation);
  };
  
  return (
    <div className="flex items-center space-x-2 mb-4">
      <span className="text-sm font-medium">Federation:</span>
      <div className="flex rounded-md shadow-sm">
        {federations.map((federation, index) => (
          <button
            key={federation}
            onClick={() => handleSelect(federation)}
            className={`px-3 py-1 text-sm font-medium ${
              selected === federation
                ? "bg-blue-600 text-white"
                : "bg-white text-gray-700 hover:bg-gray-50"
            } border border-gray-300 ${
              index === 0 ? "rounded-l-md" : ""
            } ${
              index === federations.length - 1 ? "rounded-r-md" : ""
            } ${
              index > 0 ? "border-l-0" : ""
            }`}
          >
            {federation === 'global' ? 'All Federations' : federation}
          </button>
        ))}
      </div>
    </div>
  );
} 